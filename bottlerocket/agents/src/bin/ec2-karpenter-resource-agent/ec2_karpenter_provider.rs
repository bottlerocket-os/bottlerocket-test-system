use agent_utils::aws::aws_config;
use agent_utils::json_display;
use aws_sdk_ec2::model::Tag;
use aws_sdk_eks::model::{
    NodegroupScalingConfig, NodegroupStatus, Taint, TaintEffect, UpdateTaintsPayload,
};
use bottlerocket_types::agent_config::{
    ClusterType, Ec2KarpenterConfig, AWS_CREDENTIALS_SECRET_NAME,
};
use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::Node;
use kube::config::{KubeConfigOptions, Kubeconfig};
use kube::{Api, Client, Config};
use log::{debug, info};
use resource_agent::clients::InfoClient;
use resource_agent::provider::{
    Create, Destroy, IntoProviderError, ProviderError, ProviderResult, Resources, Spec,
};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashSet;
use std::convert::TryInto;
use std::fmt::Debug;
use std::fs;
use std::process::Command;
use std::time::Duration;
use testsys_model::{Configuration, SecretName};

const KARPENTER_VERSION: &str = "v0.27.1";
const CLUSTER_KUBECONFIG: &str = "/local/cluster.kubeconfig";
const PROVISIONER_YAML: &str = "/local/provisioner.yaml";
const TAINTED_NODEGROUP_NAME: &str = "tainted-nodegroup";

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductionMemo {
    /// In this resource we put some traces here that describe what our provider is doing.
    pub current_status: String,

    /// Ids of all created ec2 instances.
    pub instance_ids: HashSet<String>,

    /// The region the clusters are in.
    pub region: String,

    /// The name of the secret containing aws credentials.
    pub aws_secret_name: Option<SecretName>,

    /// The role that is assumed.
    pub assume_role: Option<String>,

    /// The type of orchestrator the EC2 instances are connected to as workers
    pub cluster_type: ClusterType,

    /// Name of the cluster the EC2 instances are for
    pub cluster_name: String,
}

impl Configuration for ProductionMemo {}

/// Once we have fulfilled the `Create` request, we return information about the batch of ec2 instances we
/// created.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CreatedEc2Instances {}

impl Configuration for CreatedEc2Instances {}

pub struct Ec2KarpenterCreator {}

#[async_trait::async_trait]
impl Create for Ec2KarpenterCreator {
    type Config = Ec2KarpenterConfig;
    type Info = ProductionMemo;
    type Resource = CreatedEc2Instances;

    async fn create<I>(
        &self,
        spec: Spec<Self::Config>,
        client: &I,
    ) -> ProviderResult<Self::Resource>
    where
        I: InfoClient,
    {
        debug!(
            "create is starting with the following spec:\n{}",
            json_display(&spec)
        );

        let mut resources = Resources::Unknown;

        let mut memo: ProductionMemo = client
            .get_info()
            .await
            .context(resources, "Unable to get info from info client")?;

        resources = Resources::Clear;

        info!("Getting AWS secret");
        memo.current_status = "Getting AWS secret".to_string();
        client
            .send_info(memo.clone())
            .await
            .context(resources, "Error sending cluster creation message")?;

        memo.aws_secret_name = spec.secrets.get(AWS_CREDENTIALS_SECRET_NAME).cloned();
        memo.assume_role = spec.configuration.assume_role.clone();
        memo.cluster_name = spec.configuration.cluster_name.clone();

        info!("Creating AWS config");
        memo.current_status = "Creating AWS config".to_string();
        client
            .send_info(memo.clone())
            .await
            .context(resources, "Error sending cluster creation message")?;

        let shared_config = aws_config(
            &spec.secrets.get(AWS_CREDENTIALS_SECRET_NAME),
            &spec.configuration.assume_role,
            &None,
            &Some(spec.configuration.region.clone()),
            &None,
            true,
        )
        .await
        .context(resources, "Error creating config")?;
        let ec2_client = aws_sdk_ec2::Client::new(&shared_config);
        let eks_client = aws_sdk_eks::Client::new(&shared_config);
        let sts_client = aws_sdk_sts::Client::new(&shared_config);
        let iam_client = aws_sdk_iam::Client::new(&shared_config);

        info!("Writing cluster's kubeconfig to {}", CLUSTER_KUBECONFIG);
        let status = Command::new("eksctl")
            .args([
                "utils",
                "write-kubeconfig",
                "-r",
                &spec.configuration.region,
                &format!("--cluster={}", &spec.configuration.cluster_name),
                &format!("--kubeconfig={}", CLUSTER_KUBECONFIG),
            ])
            .status()
            .context(Resources::Remaining, "Failed write kubeconfig")?;

        if !status.success() {
            return Err(ProviderError::new_with_context(
                Resources::Remaining,
                format!("Failed write kubeconfig with status code {}", status),
            ));
        }

        info!("Getting the AWS account id");
        let account_id = sts_client
            .get_caller_identity()
            .send()
            .await
            .context(resources, "Unable to get caller identity")?
            .account()
            .context(resources, "The caller identity was missing an account id")?
            .to_string();
        info!("Using account '{account_id}'");

        info!("Checking for KarpenterInstanceNodeRole");
        create_karpenter_instance_role(&iam_client).await?;

        info!("Checking for KarpenterControllerPolicy");
        create_controller_policy(&iam_client, &account_id).await?;

        info!(
            "Adding associate-iam-oidc-provider to {}",
            spec.configuration.cluster_name
        );
        let status = Command::new("eksctl")
            .args([
                "utils",
                "associate-iam-oidc-provider",
                "-r",
                &spec.configuration.region,
                "--cluster",
                spec.configuration.cluster_name.as_str(),
                "--approve",
            ])
            .status()
            .context(Resources::Clear, "Failed to associate-iam-oidc-provider")?;
        if !status.success() {
            return Err(ProviderError::new_with_context(
                Resources::Clear,
                format!(
                    "Failed to associate-iam-oidc-provider with status code {}",
                    status
                ),
            ));
        }

        info!(
            "Creating iamserviceaccount for {}",
            spec.configuration.cluster_name
        );

        let status = Command::new("eksctl")
            .args([
                "create",
                "iamserviceaccount",
                "-r",
                &spec.configuration.region,
                "--cluster",
                spec.configuration.cluster_name.as_str(),
                "--name",
                "karpenter",
                "--namespace",
                "karpenter",
                "--role-name",
                format!(
                    "KarpenterControllerRole-{}",
                    &spec.configuration.cluster_name
                )
                .as_str(),
                "--attach-policy-arn",
                format!("arn:aws:iam::{account_id}:policy/KarpenterControllerPolicy").as_str(),
                "--role-only",
                "--approve",
            ])
            .status()
            .context(Resources::Clear, "Failed to create iamserviceaccount")?;
        if !status.success() {
            return Err(ProviderError::new_with_context(
                Resources::Clear,
                format!(
                    "Failed to create iamserviceaccount with status code {}",
                    status
                ),
            ));
        }

        info!(
            "Adding Karpenter tags to subnets: {:#?}",
            spec.configuration.subnet_ids
        );
        ec2_client
            .create_tags()
            .tags(
                Tag::builder()
                    .key("karpenter.sh/discovery")
                    .value(&spec.configuration.cluster_name)
                    .build(),
            )
            .set_resources(Some(spec.configuration.subnet_ids.clone()))
            .send()
            .await
            .context(resources, "Unable to tag cluster's subnets")?;

        info!(
            "Adding Karpenter tags to security group: {:#?}",
            spec.configuration.cluster_sg
        );
        ec2_client
            .create_tags()
            .tags(
                Tag::builder()
                    .key("karpenter.sh/discovery")
                    .value(&spec.configuration.cluster_name)
                    .build(),
            )
            .set_resources(Some(spec.configuration.cluster_sg.clone()))
            .send()
            .await
            .context(resources, "Unable to tag cluster's security groups")?;

        info!("Creating K8s Client from cluster kubeconfig");
        let kubeconfig = Kubeconfig::read_from(CLUSTER_KUBECONFIG).context(
            Resources::Clear,
            "Unable to create config from cluster kubeconfig",
        )?;
        let k8s_client: Client =
            Config::from_custom_kubeconfig(kubeconfig, &KubeConfigOptions::default())
                .await
                .context(resources, "Unable to convert kubeconfig")?
                .try_into()
                .context(
                    resources,
                    "Unable to create k8s client from cluster kubeconfig",
                )?;

        info!("Creating iamidentitymapping for KarpenterInstanceNodeRole");
        let status = Command::new("eksctl")
            .args([
                "create",
                "iamidentitymapping",
                "-r",
                &spec.configuration.region,
                "--cluster",
                spec.configuration.cluster_name.as_str(),
                "--arn",
                &format!("arn:aws:iam::{account_id}:role/KarpenterInstanceNodeRole"),
                "--username",
                "system:node:{{EC2PrivateDNSName}}",
                "--group",
                "system:bootstrappers",
                "--group",
                "system:nodes",
            ])
            .status()
            .context(Resources::Clear, "Failed to create iamidentitymapping")?;
        if !status.success() {
            return Err(ProviderError::new_with_context(
                Resources::Clear,
                format!(
                    "Failed to create iamidentitymapping with status code {}",
                    status
                ),
            ));
        }

        info!("Creating tainted managed nodegroup");
        let status = Command::new("eksctl")
            .args([
                "create",
                "nodegroup",
                "-r",
                &spec.configuration.region,
                "--cluster",
                spec.configuration.cluster_name.as_str(),
                "--name",
                TAINTED_NODEGROUP_NAME,
            ])
            .status()
            .context(Resources::Clear, "Failed to create nodegroup")?;
        if !status.success() {
            return Err(ProviderError::new_with_context(
                Resources::Clear,
                format!("Failed to create nodegroup with status code {}", status),
            ));
        }

        info!("Applying node taint and scaling nodegroup");
        eks_client
            .update_nodegroup_config()
            .cluster_name(&spec.configuration.cluster_name)
            .nodegroup_name(TAINTED_NODEGROUP_NAME)
            .scaling_config(
                NodegroupScalingConfig::builder()
                    .min_size(2)
                    .max_size(2)
                    .desired_size(2)
                    .build(),
            )
            // Apply a taint to prevent sonobuoy from using these nodes
            .taints(
                UpdateTaintsPayload::builder()
                    .add_or_update_taints(
                        Taint::builder()
                            .key("sonobuoy")
                            .value("ignore")
                            .effect(TaintEffect::PreferNoSchedule)
                            .build(),
                    )
                    .build(),
            )
            .send()
            .await
            .context(
                resources,
                "Unable to increase nodegroup size and apply taints",
            )?;

        info!("Creating helm template file");
        let status = Command::new("helm")
            .env("KUBECONFIG", CLUSTER_KUBECONFIG)
            .args([
                "upgrade", 
                "--install", 
                "--namespace", "karpenter",
                "--create-namespace", "karpenter", 
                "oci://public.ecr.aws/karpenter/karpenter",
                "--version", KARPENTER_VERSION,
                "--set", "settings.aws.defaultInstanceProfile=KarpenterInstanceNodeRole",
                "--set", &format!("settings.aws.clusterEndpoint={}", spec.configuration.endpoint),
                "--set", &format!("settings.aws.clusterName={}", spec.configuration.cluster_name),
                "--set", &format!(r#"serviceAccount.annotations.eks\.amazonaws\.com/role-arn=arn:aws:iam::{account_id}:role/KarpenterControllerRole-{}"#, spec.configuration.cluster_name),
                "--wait", "--debug"
            ])
            .status()
            .context(Resources::Remaining, "Failed to create helm template")?;

        if !status.success() {
            return Err(ProviderError::new_with_context(
                Resources::Remaining,
                format!(
                    "Failed to launch karpenter template with status code {}",
                    status
                ),
            ));
        }

        info!("Karpenter has been installed to the cluster. Creating EC2 provisioner");

        let requirements = if spec.configuration.instance_types.is_empty() {
            Default::default()
        } else {
            format!(
                r#"    - key: node.kubernetes.io/instance-type
      operator: In
      values: [{}]

"#,
                spec.configuration.instance_types.join(",")
            )
        };

        let block_mappings = if spec.configuration.device_mappings.is_empty() {
            Default::default()
        } else {
            spec.configuration
                .device_mappings
                .iter()
                .map(|mapping| {
                    format!(
                        r#"
    - deviceName: {}
      ebs:
        volumeType: {}
        volumeSize: {}Gi
        deleteOnTermination: {}"#,
                        mapping.name,
                        mapping.volume_type,
                        mapping.volume_size,
                        mapping.delete_on_termination
                    )
                })
                .fold(
                    r#"    blockDeviceMappings:"#.to_string(),
                    |mappings, mapping| mappings + &mapping,
                )
        };

        let provisioner = format!(
            r#"apiVersion: karpenter.sh/v1alpha5
kind: Provisioner
metadata:
    name: default
spec:
    ttlSecondsAfterEmpty: 1   
    providerRef:
        name: my-provider
    requirements:
    - key: kubernetes.io/arch
      operator: In
      values: ["arm64", "amd64"]
{}
---
apiVersion: karpenter.k8s.aws/v1alpha1
kind: AWSNodeTemplate
metadata:
    name: my-provider
spec:
    amiFamily: Bottlerocket
    amiSelector: 
      aws-ids: {}
    subnetSelector:
        karpenter.sh/discovery: {}
    securityGroupSelector:
        karpenter.sh/discovery: {}
{}
"#,
            requirements,
            spec.configuration.node_ami,
            spec.configuration.cluster_name,
            spec.configuration.cluster_name,
            block_mappings,
        );

        info!("Writing provisioner yaml: \n {}", provisioner);

        fs::write(PROVISIONER_YAML, provisioner)
            .context(resources, "Unable to write provisioner yaml")?;

        let status = Command::new("kubectl")
            .args([
                "--kubeconfig",
                CLUSTER_KUBECONFIG,
                "apply",
                "-f",
                PROVISIONER_YAML,
            ])
            .status()
            .context(Resources::Clear, "Failed to apply provisioner")?;
        if !status.success() {
            return Err(ProviderError::new_with_context(
                Resources::Clear,
                format!("Failed to apply provisioner with status code {}", status),
            ));
        }

        // Get the current number of nodes so we know once karpenter has started scaling
        let node_api = Api::<Node>::all(k8s_client.clone());
        let nodes = node_api
            .list(&Default::default())
            .await
            .context(resources, "Unable to list nodes in target cluster.")?
            .iter()
            .count();

        info!("Creating deployment to scale karpenter nodes");

        let deployment: Deployment = serde_yaml::from_str(
            r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: inflate
  namespace: default
spec:
  replicas: 20
  selector:
    matchLabels:
      app: inflate
  template:
    metadata:
      labels:
        app: inflate
    spec:
      terminationGracePeriodSeconds: 0
      containers:
        - name: inflate
          image: "public.ecr.aws/eks-distro/kubernetes/pause:3.7"
          resources:
            requests:
              cpu: 1
"#,
        )
        .context(resources, "Unable to serialize inflate deployment")?;

        let deployment_api = Api::<Deployment>::namespaced(k8s_client, "default");
        deployment_api
            .create(&Default::default(), &deployment)
            .await
            .context(resources, "Unable to create deployment")?;

        info!("Waiting for new nodes to be created");
        tokio::time::timeout(
            Duration::from_secs(600),
            wait_for_nodes(&node_api, nodes, Ordering::Greater),
        )
        .await
        .context(
            resources,
            "Timed out waiting for karpenter nodes to join the cluster",
        )??;

        // make nodes no schedule
        info!("Waiting for tainted nodegroup to become active");
        tokio::time::timeout(
            Duration::from_secs(600),
            wait_for_nodegroup(
                &eks_client,
                &spec.configuration.cluster_name,
                TAINTED_NODEGROUP_NAME,
            ),
        )
        .await
        .context(
            resources,
            "Timed out waiting for tainted nodegroup to be `ACTIVE`",
        )??;
        info!("Making tainted nodegroup unschedulable");
        eks_client
            .update_nodegroup_config()
            .cluster_name(&spec.configuration.cluster_name)
            .nodegroup_name(TAINTED_NODEGROUP_NAME)
            // Apply a taint to prevent sonobuoy from using these nodes
            .taints(
                UpdateTaintsPayload::builder()
                    .add_or_update_taints(
                        Taint::builder()
                            .key("sonobuoy")
                            .value("ignore")
                            .effect(TaintEffect::NoSchedule)
                            .build(),
                    )
                    .build(),
            )
            .send()
            .await
            .context(resources, "Unable to apply taints")?;

        // watch for 2 nodes to have no schedule
        info!("Waiting for nodes to be tainted");
        tokio::time::timeout(
            Duration::from_secs(600),
            wait_for_tainted_nodes(&node_api, nodes),
        )
        .await
        .context(
            resources,
            "Timed out waiting for karpenter nodes to join the cluster",
        )??;

        Ok(CreatedEc2Instances {})
    }
}

/// Loop until the number of nodes in the cluster is greater than `current_count`
async fn wait_for_nodes(
    api: &Api<Node>,
    current_count: usize,
    comp: Ordering,
) -> ProviderResult<()> {
    let cmp_string = match &comp {
        Ordering::Less => "less than",
        Ordering::Equal => "exactly",
        Ordering::Greater => "more than",
    };
    info!(
        "Checking for {} {current_count} nodes in the cluster",
        cmp_string
    );
    loop {
        let nodes = api
            .list(&Default::default())
            .await
            .context(
                Resources::Remaining,
                "Unable to list nodes in target cluster.",
            )?
            .iter()
            .count();
        if nodes.cmp(&current_count) == comp {
            info!("Expected node count has been reached");
            return Ok(());
        }
        info!("Found '{}' nodes", nodes);
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

/// Loop until the number of nodes with `sonobuoy=ignore:NoSchedule` taint is `desired_count`
async fn wait_for_tainted_nodes(api: &Api<Node>, desired_count: usize) -> ProviderResult<()> {
    loop {
        info!("Checking for tainted nodes in the cluster");
        let nodes = api
            .list(&Default::default())
            .await
            .context(
                Resources::Remaining,
                "Unable to list nodes in target cluster.",
            )?
            .iter()
            .filter_map(|node| node.spec.as_ref())
            .filter_map(|spec| spec.taints.as_ref())
            .filter(|taints| {
                taints.iter().any(|taint| {
                    taint.key.as_str() == "sonobuoy"
                        && taint.value == Some("ignore".to_string())
                        && taint.effect.as_str() == "NoSchedule"
                })
            })
            .count();
        if nodes >= desired_count {
            info!("All nodes have the new taint");
            return Ok(());
        }
        info!(
            "'{}' of '{}' nodes have the sonobuoy=ignore:NoSchedule taint. Sleeping 5s",
            nodes, desired_count
        );
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

/// Loop until the specified nodegroup is active
async fn wait_for_nodegroup(
    eks_client: &aws_sdk_eks::Client,
    cluster: &str,
    nodegroup: &str,
) -> ProviderResult<()> {
    loop {
        let status = eks_client
            .describe_nodegroup()
            .cluster_name(cluster)
            .nodegroup_name(nodegroup)
            .send()
            .await
            .context(Resources::Remaining, "Unable to describe nodegroup")?
            .nodegroup()
            .context(Resources::Remaining, "No nodegroup was found")?
            .status()
            .context(Resources::Remaining, "The nodegroup did not have a status")?
            .to_owned();
        if matches!(status, NodegroupStatus::Active) {
            info!("The nodegroup '{}' is now active", nodegroup);
            return Ok(());
        }
        info!(
            "The nodegroup '{}' is currently '{:?}'. Sleeping 5s",
            nodegroup, status
        );
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn create_karpenter_instance_role(iam_client: &aws_sdk_iam::Client) -> ProviderResult<()> {
    if iam_client
        .get_instance_profile()
        .instance_profile_name("KarpenterInstanceNodeRole")
        .send()
        .await
        .map(|instance_profile| instance_profile.instance_profile().is_some())
        .unwrap_or_default()
    {
        info!("KarpenterInstanceNodeRole instance profile already exists");
        return Ok(());
    }

    if iam_client
        .get_role()
        .role_name("KarpenterInstanceNodeRole")
        .send()
        .await
        .is_ok()
    {
        info!("KarpenterInstanceNodeRole already exists");
    } else {
        info!("Creating karpenter instance role");
        iam_client
            .create_role()
            .role_name("KarpenterInstanceNodeRole")
            .assume_role_policy_document(
                r#"{
        "Version": "2012-10-17",
        "Statement": [
            {
            "Effect": "Allow",
            "Principal": {
                "Service": "ec2.amazonaws.com"
            },
            "Action": "sts:AssumeRole"
            }
        ]
    }"#,
            )
            .send()
            .await
            .context(
                Resources::Clear,
                "Unable to create KarpenterInstanceNodeRole",
            )?;
        let policies = vec![
            "AmazonEKSWorkerNodePolicy",
            "AmazonEKS_CNI_Policy",
            "AmazonEC2ContainerRegistryReadOnly",
            "AmazonSSMManagedInstanceCore",
        ];
        for policy in policies {
            iam_client
                .attach_role_policy()
                .role_name("KarpenterInstanceNodeRole")
                .policy_arn(format!("arn:aws:iam::aws:policy/{}", policy))
                .send()
                .await
                .context(
                    Resources::Clear,
                    format!(
                        "Unable to add policy {} to KarpenterInstanceNodeRole",
                        policy
                    ),
                )?;
        }
    }

    info!("Creating instance profile: 'KarpenterInstanceNodeRole'");
    iam_client
        .create_instance_profile()
        .instance_profile_name("KarpenterInstanceNodeRole")
        .send()
        .await
        .context(Resources::Clear, "Unable to create instance profile")?;

    iam_client
        .add_role_to_instance_profile()
        .instance_profile_name("KarpenterInstanceNodeRole")
        .role_name("KarpenterInstanceNodeRole")
        .send()
        .await
        .context(Resources::Clear, "Unable to add role to InstanceProfile")?;

    Ok(())
}

async fn create_controller_policy(
    iam_client: &aws_sdk_iam::Client,
    account_id: &str,
) -> ProviderResult<()> {
    if iam_client
        .get_policy()
        .policy_arn(format!(
            "arn:aws:iam::{}:policy/KarpenterControllerPolicy",
            account_id
        ))
        .send()
        .await
        .is_ok()
    {
        info!("KarpenterControllerPolicy already exists");
        return Ok(());
    }

    info!("Creating controller policy");
    iam_client
        .create_policy()
        .policy_name("KarpenterControllerPolicy")
        .policy_document(
            r#"{
        "Statement": [
            {
                "Action": [
                    "ssm:GetParameter",
                    "iam:PassRole",
                    "ec2:DescribeImages",
                    "ec2:RunInstances",
                    "ec2:DescribeSubnets",
                    "ec2:DescribeSecurityGroups",
                    "ec2:DescribeLaunchTemplates",
                    "ec2:DescribeInstances",
                    "ec2:DescribeInstanceTypes",
                    "ec2:DescribeInstanceTypeOfferings",
                    "ec2:DescribeAvailabilityZones",
                    "ec2:DeleteLaunchTemplate",
                    "ec2:CreateTags",
                    "ec2:CreateLaunchTemplate",
                    "ec2:CreateFleet",
                    "ec2:DescribeSpotPriceHistory",
                    "pricing:GetProducts"
                ],
                "Effect": "Allow",
                "Resource": "*",
                "Sid": "Karpenter"
            },
            {
                "Action": "ec2:TerminateInstances",
                "Condition": {
                    "StringLike": {
                        "ec2:ResourceTag/Name": "*karpenter*"
                    }
                },
                "Effect": "Allow",
                "Resource": "*",
                "Sid": "ConditionalEC2Termination"
            }
        ],
        "Version": "2012-10-17"
    }"#,
        )
        .send()
        .await
        .context(
            Resources::Clear,
            "Unable to create Karpenter controller policy",
        )?;
    Ok(())
}

/// This is the object that will destroy ec2 instances.
pub struct Ec2KarpenterDestroyer {}

#[async_trait::async_trait]
impl Destroy for Ec2KarpenterDestroyer {
    type Config = Ec2KarpenterConfig;
    type Info = ProductionMemo;
    type Resource = CreatedEc2Instances;

    async fn destroy<I>(
        &self,
        maybe_spec: Option<Spec<Self::Config>>,
        _resource: Option<Self::Resource>,
        client: &I,
    ) -> ProviderResult<()>
    where
        I: InfoClient,
    {
        let spec = maybe_spec.context(Resources::Remaining, "Unable to get the resource spec")?;
        let mut memo: ProductionMemo = client.get_info().await.map_err(|e| {
            ProviderError::new_with_source_and_context(
                Resources::Unknown,
                "Unable to get info from client",
                e,
            )
        })?;
        let resources = Resources::Remaining;

        info!("Getting AWS secret");
        memo.current_status = "Getting AWS secret".to_string();
        client
            .send_info(memo.clone())
            .await
            .context(resources, "Error sending cluster creation message")?;

        memo.aws_secret_name = spec.secrets.get(AWS_CREDENTIALS_SECRET_NAME).cloned();
        memo.assume_role = spec.configuration.assume_role.clone();
        memo.cluster_name = spec.configuration.cluster_name.clone();

        info!("Creating AWS config");
        memo.current_status = "Creating AWS config".to_string();
        client
            .send_info(memo.clone())
            .await
            .context(resources, "Error sending cluster creation message")?;

        let shared_config = aws_config(
            &spec.secrets.get(AWS_CREDENTIALS_SECRET_NAME),
            &spec.configuration.assume_role,
            &None,
            &Some(spec.configuration.region.clone()),
            &None,
            true,
        )
        .await
        .context(resources, "Error creating config")?;

        let eks_client = aws_sdk_eks::Client::new(&shared_config);

        info!("Writing cluster's kubeconfig to {}", CLUSTER_KUBECONFIG);
        let status = Command::new("eksctl")
            .args([
                "utils",
                "write-kubeconfig",
                "-r",
                &spec.configuration.region,
                &format!("--cluster={}", &spec.configuration.cluster_name),
                &format!("--kubeconfig={}", CLUSTER_KUBECONFIG),
            ])
            .status()
            .context(Resources::Remaining, "Failed write kubeconfig")?;

        if !status.success() {
            return Err(ProviderError::new_with_context(
                Resources::Remaining,
                format!("Failed write kubeconfig with status code {}", status),
            ));
        }

        info!("Removing taint from tainted nodegroup");
        eks_client
            .update_nodegroup_config()
            .cluster_name(&spec.configuration.cluster_name)
            .nodegroup_name(TAINTED_NODEGROUP_NAME)
            .taints(
                UpdateTaintsPayload::builder()
                    .remove_taints(
                        Taint::builder()
                            .key("sonobuoy")
                            .value("ignore")
                            .effect(TaintEffect::NoSchedule)
                            .build(),
                    )
                    .build(),
            )
            .send()
            .await
            .context(resources, "Unable to apply taints")?;

        info!("Creating K8s Client from cluster kubeconfig");
        let kubeconfig = Kubeconfig::read_from(CLUSTER_KUBECONFIG)
            .context(resources, "Unable to create config from cluster kubeconfig")?;
        let k8s_client: Client =
            Config::from_custom_kubeconfig(kubeconfig, &KubeConfigOptions::default())
                .await
                .context(resources, "Unable to convert kubeconfig")?
                .try_into()
                .context(
                    resources,
                    "Unable to create k8s client from cluster kubeconfig",
                )?;

        info!("Deleting inflate deployment");
        let deployment_api = Api::<Deployment>::namespaced(k8s_client.clone(), "default");
        deployment_api
            .delete("inflate", &Default::default())
            .await
            .context(resources, "Unable to delete deployment")?;

        let node_api = Api::<Node>::all(k8s_client);

        info!("Waiting for karpenter nodes to be cleaned up");
        tokio::time::timeout(
            Duration::from_secs(600),
            wait_for_nodes(&node_api, 2, Ordering::Equal),
        )
        .await
        .context(
            resources,
            "Timed out waiting for karpenter nodes to join the cluster",
        )??;

        info!("Uninstalling karpenter");
        let status = Command::new("helm")
            .env("KUBECONFIG", CLUSTER_KUBECONFIG)
            .args(["uninstall", "karpenter", "--namespace", "karpenter"])
            .status()
            .context(Resources::Remaining, "Failed to create helm template")?;

        if !status.success() {
            return Err(ProviderError::new_with_context(
                Resources::Remaining,
                format!(
                    "Failed to launch karpenter template with status code {}",
                    status
                ),
            ));
        }

        info!("Deleting tainted nodegroup");
        let status = Command::new("eksctl")
            .args([
                "delete",
                "nodegroup",
                "-r",
                &spec.configuration.region,
                "--cluster",
                spec.configuration.cluster_name.as_str(),
                "--name",
                TAINTED_NODEGROUP_NAME,
                "--wait",
            ])
            .status()
            .context(resources, "Failed to delete nodegroup")?;
        if !status.success() {
            return Err(ProviderError::new_with_context(
                resources,
                format!("Failed to delete nodegroup with status code {}", status),
            ));
        }

        memo.current_status = "Instances deleted".into();
        client.send_info(memo.clone()).await.map_err(|e| {
            ProviderError::new_with_source_and_context(
                resources,
                "Error sending final destruction message",
                e,
            )
        })?;

        Ok(())
    }
}
