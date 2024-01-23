use agent_utils::aws::aws_config;
use agent_utils::json_display;
use aws_sdk_cloudformation::model::{Capability, Parameter, StackStatus};
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
use log::{debug, info, warn};
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
use tokio::fs::read_to_string;

const KARPENTER_VERSION: &str = "v0.33.1";
const CLUSTER_KUBECONFIG: &str = "/local/cluster.kubeconfig";
const PROVISIONER_YAML: &str = "/local/provisioner.yaml";
const TAINTED_NODEGROUP_NAME: &str = "tainted-nodegroup";
const TEMPLATE_PATH: &str = "/local/cloudformation.yaml";

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

    pub cloud_formation_stack_exists: bool,
    pub tainted_nodegroup_exists: bool,
    pub karpenter_namespace_exists: bool,
    pub inflate_deployment_exists: bool,
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

        let karpenter_version = spec
            .configuration
            .karpenter_version
            .unwrap_or_else(|| KARPENTER_VERSION.to_string());

        let stack_name = format!("Karpenter-{}", spec.configuration.cluster_name);

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
        let cfn_client = aws_sdk_cloudformation::Client::new(&shared_config);

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

        memo.cloud_formation_stack_exists = true;
        client
            .send_info(memo.clone())
            .await
            .context(resources, "Error sending cluster creation message")?;

        info!("Launching karpenter cloud formation stack");
        cfn_client
            .create_stack()
            .stack_name(&stack_name)
            .template_body(
                read_to_string(TEMPLATE_PATH)
                    .await
                    .context(Resources::Clear, "Unable to read cloudformation template")?,
            )
            .capabilities(Capability::CapabilityNamedIam)
            .parameters(
                Parameter::builder()
                    .parameter_key("ClusterName")
                    .parameter_value(&spec.configuration.cluster_name)
                    .build(),
            )
            .send()
            .await
            .context(
                Resources::Remaining,
                "Unable to create cloudformation stack",
            )?;

        tokio::time::timeout(
            Duration::from_secs(600),
            wait_for_cloudformation_stack(
                stack_name.to_string(),
                StackStatus::CreateComplete,
                &cfn_client,
            ),
        )
        .await
        .context(resources, "Timed out waiting for cloud formation stack.")??;

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
                format!(
                    "arn:aws:iam::{account_id}:policy/KarpenterControllerPolicy-{}",
                    &spec.configuration.cluster_name
                )
                .as_str(),
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
            .set_resources(Some(vec![spec.configuration.cluster_sg.clone()]))
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
                &format!(
                    "arn:aws:iam::{account_id}:role/KarpenterNodeRole-{}",
                    spec.configuration.cluster_name
                ),
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
        memo.tainted_nodegroup_exists = true;
        memo.current_status = "Tainting nodegroup".to_string();
        client
            .send_info(memo.clone())
            .await
            .context(resources, "Error sending message")?;

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
                "karpenter",
                "--namespace",
                "karpenter",
                "--create-namespace",
                "oci://public.ecr.aws/karpenter/karpenter",
                "--version",
                &karpenter_version,
                "--set",
                &format!(
                    "aws.defaultInstanceProfile=KarpenterNodeRole-{}",
                    spec.configuration.cluster_name
                ),
                "--set",
                &format!("settings.clusterName={}", spec.configuration.cluster_name),
                "--set",
                &format!(
                    "settings.aws.clusterEndpoint={}",
                    spec.configuration.endpoint
                ),
                "--set", &format!(r#"serviceAccount.annotations.eks\.amazonaws\.com/role-arn=arn:aws:iam::{account_id}:role/KarpenterControllerRole-{}"#, spec.configuration.cluster_name),
                "--wait",
                "--debug",
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

        memo.karpenter_namespace_exists = true;
        memo.current_status = "Karpenter Installed".to_string();
        client
            .send_info(memo.clone())
            .await
            .context(resources, "Error sending message")?;

        info!("Karpenter has been installed to the cluster. Creating EC2 provisioner");

        let requirements = if spec.configuration.instance_types.is_empty() {
            Default::default()
        } else {
            format!(
                r#"        - key: node.kubernetes.io/instance-type
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
            r#"apiVersion: karpenter.sh/v1beta1
kind: NodePool
metadata:
    name: default
spec: 
  template:
    spec:
        nodeClassRef:
            name: my-provider
        requirements:
        - key: kubernetes.io/arch
          operator: In
          values: ["arm64", "amd64"]
{}
---
apiVersion: karpenter.k8s.aws/v1beta1
kind: EC2NodeClass
metadata:
    name: my-provider
spec:
    amiFamily: Bottlerocket
    role: "KarpenterNodeRole-{}"
    amiSelectorTerms: 
      - id: {}
    subnetSelectorTerms:
        - tags:
            karpenter.sh/discovery: {}
    securityGroupSelectorTerms:
        - tags:
            karpenter.sh/discovery: {}
{}
"#,
            requirements,
            spec.configuration.cluster_name,
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

        memo.inflate_deployment_exists = true;
        memo.current_status = "Waiting for Karpenter Nodes".to_string();
        client
            .send_info(memo.clone())
            .await
            .context(resources, "Error sending message")?;

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

        if !memo.cloud_formation_stack_exists {
            return Ok(());
        }

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
        if memo.tainted_nodegroup_exists {
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

            info!("Checking that tainted nodegroup is ready");
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

            if memo.inflate_deployment_exists {
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
                    "Timed out waiting for karpenter nodes to leave the cluster",
                )??;
            }

            if memo.karpenter_namespace_exists {
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
                    "--disable-eviction",
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
        }

        // Remove the instance profile from the karpenter role
        let iam_client = aws_sdk_iam::Client::new(&shared_config);
        let instance_profile_out = iam_client
            .list_instance_profiles_for_role()
            .role_name(format!(
                "KarpenterNodeRole-{}",
                spec.configuration.cluster_name
            ))
            .send()
            .await
            .context(Resources::Remaining, "Unable to list instance profiles")?;
        let instance_profile = instance_profile_out
            .instance_profiles()
            .and_then(|profiles| profiles.first())
            .and_then(|instance_profile| instance_profile.instance_profile_name().to_owned());

        if let Some(instance_profile) = instance_profile {
            iam_client
                .remove_role_from_instance_profile()
                .instance_profile_name(instance_profile)
                .role_name(format!(
                    "KarpenterNodeRole-{}",
                    spec.configuration.cluster_name
                ))
                .send()
                .await
                .context(
                    Resources::Remaining,
                    "Unable to remove role from instance profile",
                )?;
        }

        let status = Command::new("eksctl")
            .args([
                "delete",
                "iamserviceaccount",
                "-r",
                &spec.configuration.region,
                "--cluster",
                spec.configuration.cluster_name.as_str(),
                "--name",
                "karpenter",
                "--namespace",
                "karpenter",
                "--wait",
            ])
            .status();
        if status.is_err() {
            warn!("Unable to delete service account. It is possible it was already deleted.");
        }

        let status = iam_client
            .delete_role()
            .role_name(format!(
                "KarpenterControllerRole-{}",
                &spec.configuration.cluster_name
            ))
            .send()
            .await;
        if status.is_err() {
            warn!("Unable to Karpenter controller role. It is possible it was already deleted.");
        }

        let stack_name = format!("Karpenter-{}", spec.configuration.cluster_name);
        let cfn_client = aws_sdk_cloudformation::Client::new(&shared_config);

        cfn_client
            .delete_stack()
            .stack_name(&stack_name)
            .send()
            .await
            .context(
                Resources::Remaining,
                "Unable to delete cloudformation stack",
            )?;

        let _ = tokio::time::timeout(
            Duration::from_secs(600),
            wait_for_cloudformation_stack_deletion(stack_name, &cfn_client),
        )
        .await
        .context(
            Resources::Remaining,
            "Timed out waiting for cloud formation stack to delete.",
        )?;

        Ok(())
    }
}

async fn wait_for_cloudformation_stack(
    stack_name: String,
    desired_state: StackStatus,
    cfn_client: &aws_sdk_cloudformation::Client,
) -> ProviderResult<()> {
    let mut state = StackStatus::CreateInProgress;
    while state != desired_state {
        info!(
            "Waiting for cloudformation stack '{}' to reach '{:?}' state",
            stack_name, desired_state
        );
        state = cfn_client
            .describe_stacks()
            .stack_name(&stack_name)
            .send()
            .await
            .context(Resources::Remaining, "Unable to describe stack")?
            .stacks
            .and_then(|stacks| stacks.into_iter().next())
            .and_then(|stack| stack.stack_status)
            .unwrap_or(StackStatus::CreateInProgress);
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    Ok(())
}

async fn wait_for_cloudformation_stack_deletion(
    stack_name: String,
    cfn_client: &aws_sdk_cloudformation::Client,
) -> ProviderResult<()> {
    loop {
        info!(
            "Waiting for cloudformation stack '{}' to be deleted",
            stack_name
        );
        if cfn_client
            .describe_stacks()
            .stack_name(&stack_name)
            .send()
            .await
            .context(Resources::Remaining, "Unable to describe stack")?
            .stacks()
            .map(|s| s.is_empty())
            .unwrap_or_default()
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
