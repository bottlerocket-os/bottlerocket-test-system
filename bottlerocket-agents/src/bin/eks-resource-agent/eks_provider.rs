use aws_config::meta::region::RegionProviderChain;
use aws_sdk_ec2::model::{Filter, SecurityGroup, Subnet};
use aws_sdk_ec2::{Region, SdkError};
use aws_sdk_eks::error::{DescribeClusterError, DescribeClusterErrorKind};
use aws_sdk_eks::output::DescribeClusterOutput;
use bottlerocket_agents::{
    impl_display_as_json, json_display, K8sVersion, AWS_CREDENTIALS_SECRET_NAME,
};
use log::{debug, info, trace};
use model::{Configuration, SecretName};
use resource_agent::clients::InfoClient;
use resource_agent::provider::{
    Create, Destroy, IntoProviderError, ProviderError, ProviderResult, Resources, Spec,
};
use serde::{Deserialize, Serialize};
use std::env::{self, temp_dir};
use std::path::Path;
use std::process::Command;

/// The default region for the cluster.
const DEFAULT_REGION: &str = "us-west-2";
/// The default cluster version.
const DEFAULT_VERSION: &str = "1.21";

/// The configuration information for a eks instance provider.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClusterConfig {
    /// The name of the eks cluster to create or an existing cluster.
    cluster_name: String,

    /// Whether this agent will create the cluster or not.
    creation_policy: Option<CreationPolicy>,

    /// The AWS region to create the cluster. If no value is provided `us-west-2` will be used.
    region: Option<String>,

    /// The availability zones. (e.g. us-west-2a,us-west-2b)
    zones: Option<Vec<String>>,

    /// The eks version of the the cluster (e.g. "1.14", "1.15", "1.16"). Make sure this is
    /// quoted so that it is interpreted as a JSON/YAML string (not a number).
    version: Option<K8sVersion>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CreationPolicy {
    /// Create the cluster, it is an error if the cluster already exists. This is the default
    /// behavior when no `CreationPolicy` is provided.
    Create,
    /// Create the cluster if it does not already exist.
    IfNotExists,
    /// Never create the cluster, it is an error if it does not exist.
    Never,
}

impl Default for CreationPolicy {
    fn default() -> Self {
        Self::Create
    }
}

impl Configuration for ClusterConfig {}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductionMemo {
    pub current_status: String,

    /// The name of the secret containing aws credentials.
    pub aws_secret_name: Option<SecretName>,

    /// The name of the cluster we created.
    pub cluster_name: Option<String>,

    /// Whether the agent was instructed to create the cluster or not.
    pub creation_policy: Option<CreationPolicy>,

    // The region the cluster is in.
    pub region: Option<String>,
}

impl Configuration for ProductionMemo {}
impl_display_as_json!(ProductionMemo);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CreatedCluster {
    /// The name of the cluster we created.
    pub cluster_name: String,

    /// The regions of the cluster.
    pub region: String,

    /// The eks server endpoint.
    pub endpoint: String,

    /// The cluster certificate.
    pub certificate: String,

    /// All public subnet ids.
    pub public_subnet_ids: Vec<String>,
    /// A single public subnet id. Will be `None` if no public ids exist.
    pub public_subnet_id: Option<String>,

    /// All private subnet ids.
    pub private_subnet_ids: Vec<String>,
    /// A single private subnet id. Will be `None` if no private ids exist.
    pub private_subnet_id: Option<String>,

    /// Security groups necessary for ec2 instances
    pub security_groups: Vec<String>,
    pub nodegroup_sg: Vec<String>,
    pub controlplane_sg: Vec<String>,
    pub clustershared_sg: Vec<String>,

    /// The eksctl create iam instance profile.
    pub iam_instance_profile_arn: String,

    // Base64 encoded kubeconfig
    pub encoded_kubeconfig: String,
}

impl Configuration for CreatedCluster {}
impl_display_as_json!(CreatedCluster);

#[derive(Debug)]
struct AwsClients {
    eks_client: aws_sdk_eks::Client,
    ec2_client: aws_sdk_ec2::Client,
    iam_client: aws_sdk_iam::Client,
}

impl AwsClients {
    async fn new(region: String) -> Self {
        let region_provider = RegionProviderChain::first_try(Some(Region::new(region)));
        let shared_config = aws_config::from_env().region(region_provider).load().await;
        Self {
            eks_client: aws_sdk_eks::Client::new(&shared_config),
            ec2_client: aws_sdk_ec2::Client::new(&shared_config),
            iam_client: aws_sdk_iam::Client::new(&shared_config),
        }
    }
}

pub struct EksCreator {}

#[async_trait::async_trait]
impl Create for EksCreator {
    type Config = ClusterConfig;
    type Info = ProductionMemo;
    type Resource = CreatedCluster;

    async fn create<I>(
        &self,
        spec: Spec<Self::Config>,
        client: &I,
    ) -> ProviderResult<Self::Resource>
    where
        I: InfoClient,
    {
        debug!("Create starting with spec: \n{}", json_display(&spec));
        let mut memo: ProductionMemo = client
            .get_info()
            .await
            .context(Resources::Clear, "Unable to get info from client")?;
        memo.creation_policy = Some(spec.configuration.creation_policy.unwrap_or_default());
        info!(
            "Beginning creation of EKS cluster '{}' with creation policy '{:?}'",
            spec.configuration.cluster_name, memo.creation_policy
        );

        let region = spec
            .configuration
            .region
            .as_ref()
            .unwrap_or(&DEFAULT_REGION.to_string())
            .to_string();

        // Write aws credentials if we need them so we can run eksctl
        if let Some(aws_secret_name) = spec.secrets.get(AWS_CREDENTIALS_SECRET_NAME) {
            setup_env(client, aws_secret_name, Resources::Clear).await?;
            memo.aws_secret_name = Some(aws_secret_name.clone());
        }
        let aws_clients = AwsClients::new(region.clone()).await;

        let (do_create, message) = is_cluster_creation_required(
            &spec.configuration.cluster_name,
            spec.configuration.creation_policy.unwrap_or_default(),
            &aws_clients,
        )
        .await?;
        memo.current_status = message;
        info!("{}", memo.current_status);
        client
            .send_info(memo.clone())
            .await
            .context(Resources::Clear, "Error sending cluster creation message")?;

        let kubeconfig_dir = temp_dir().join("kubeconfig.yaml");

        if do_create {
            info!("Creating cluster with eksctl");
            create_cluster(
                &spec.configuration.cluster_name,
                &region,
                &spec.configuration.zones,
                &spec.configuration.version,
                &kubeconfig_dir,
            )?;
            info!("Done creating cluster with eksctl");
        }

        write_kubeconfig(&spec.configuration.cluster_name, &region, &kubeconfig_dir)?;
        let kubeconfig = std::fs::read_to_string(kubeconfig_dir)
            .context(Resources::Remaining, "Unable to read kubeconfig.")?;
        let encoded_kubeconfig = base64::encode(kubeconfig);

        info!("Gathering information about the cluster");
        let created_cluster = created_cluster(
            encoded_kubeconfig,
            &spec.configuration.cluster_name,
            &region,
            &aws_clients,
        )
        .await?;

        memo.current_status = "Cluster ready".into();
        memo.cluster_name = Some(spec.configuration.cluster_name);
        memo.region = Some(region);
        debug!("Sending memo:\n{}", &memo);
        client.send_info(memo.clone()).await.context(
            Resources::Remaining,
            "Error sending cluster created message",
        )?;

        info!("Done");
        debug!("CreatedCluster: \n{}", created_cluster);
        Ok(created_cluster)
    }
}

async fn setup_env<I>(
    client: &I,
    aws_secret_name: &SecretName,
    failure_resources: Resources,
) -> ProviderResult<()>
where
    I: InfoClient,
{
    info!("Setting up AWS environment");
    let aws_secret = client.get_secret(aws_secret_name).await.context(
        failure_resources,
        format!("Error getting secret '{}'", aws_secret_name),
    )?;

    let access_key_id = String::from_utf8(
        aws_secret
            .get("access-key-id")
            .context(
                failure_resources,
                format!("access-key-id missing from secret '{}'", aws_secret_name),
            )?
            .to_owned(),
    )
    .context(
        failure_resources,
        "Could not convert access-key-id to String",
    )?;
    let secret_access_key = String::from_utf8(
        aws_secret
            .get("secret-access-key")
            .context(
                failure_resources,
                format!(
                    "secret-access-key missing from secret '{}'",
                    aws_secret_name
                ),
            )?
            .to_owned(),
    )
    .context(
        Resources::Clear,
        "Could not convert secret-access-key to String",
    )?;

    env::set_var("AWS_ACCESS_KEY_ID", access_key_id);
    env::set_var("AWS_SECRET_ACCESS_KEY", secret_access_key);

    Ok(())
}

/// Returns the bool telling us whether or not we need to create the cluster, and a string that
/// explains why that we can use for logging or memo status. Returns an error if the creation policy
/// requires us to create a cluster when it already exists, or creation policy forbids us to create
/// a cluster and it does not exist.
async fn is_cluster_creation_required(
    cluster_name: &str,
    creation_policy: CreationPolicy,
    aws_clients: &AwsClients,
) -> ProviderResult<(bool, String)> {
    let cluster_exists: bool = does_cluster_exist(cluster_name, aws_clients).await?;
    match creation_policy {
        CreationPolicy::Create if cluster_exists =>
            Err(
                ProviderError::new_with_context(
                    Resources::Clear, format!(
                        "The cluster '{}' already existed and creation policy '{:?}' requires that it not exist",
                        cluster_name,
                        creation_policy
                    )
                )
            ),
        CreationPolicy::Never if !cluster_exists => return Err(
            ProviderError::new_with_context(
                Resources::Clear, format!(
                    "The cluster '{}' does not exist and creation policy '{:?}' requires that it exist",
                    cluster_name,
                    creation_policy
                )
            )
        ),
        CreationPolicy::Create  =>{
            Ok((true, format!("Creation policy is '{:?}' and cluster '{}' does not exist: creating cluster", creation_policy, cluster_name)))
        },
        CreationPolicy::IfNotExists if !cluster_exists => {
            Ok((true, format!("Creation policy is '{:?}' and cluster '{}' does not exist: creating cluster", creation_policy, cluster_name)))
        },
        CreationPolicy::IfNotExists |
        CreationPolicy::Never => {
            Ok((false, format!("Creation policy is '{:?}' and cluster '{}' exists: not creating cluster", creation_policy, cluster_name)))
        },
    }
}

fn create_cluster(
    cluster_name: &str,
    region: &str,
    zones: &Option<Vec<String>>,
    version: &Option<K8sVersion>,
    kubeconfig_dir: &Path,
) -> ProviderResult<()> {
    let version_arg = version
        .as_ref()
        .map(|version| version.major_minor_without_v())
        .unwrap_or_else(|| DEFAULT_VERSION.to_string());

    trace!("Calling eksctl create cluster");
    let status = Command::new("eksctl")
        .args([
            "create",
            "cluster",
            "-r",
            region,
            "--zones",
            &zones.clone().unwrap_or_default().join(","),
            "--version",
            &version_arg,
            "--kubeconfig",
            kubeconfig_dir.to_str().context(
                Resources::Clear,
                format!("Unable to convert '{:?}' to string path", kubeconfig_dir),
            )?,
            "-n",
            cluster_name,
            "--nodes",
            "0",
            "--managed=false",
        ])
        .status()
        .context(Resources::Clear, "Failed create cluster")?;
    trace!("eksctl create cluster has completed");

    if !status.success() {
        return Err(ProviderError::new_with_context(
            Resources::Clear,
            format!("Failed create cluster with status code {}", status),
        ));
    }

    Ok(())
}

fn cluster_iam_identity_mapping(cluster_name: &str, region: &str) -> ProviderResult<String> {
    info!("Getting cluster role ARN");
    let iam_identity_output = Command::new("eksctl")
        .args([
            "get",
            "iamidentitymapping",
            "--cluster",
            cluster_name,
            "--region",
            region,
            "--output",
            "json",
        ])
        .output()
        .context(Resources::Remaining, "Unable to get iam identity mapping.")?;

    let iam_identity: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&iam_identity_output.stdout)).context(
            Resources::Remaining,
            "Unable to deserialize iam identity mapping",
        )?;

    iam_identity
        .get(0)
        .context(Resources::Remaining, "No profiles found.")?
        .get("rolearn")
        .context(Resources::Remaining, "Profile does not contain rolearn.")?
        .as_str()
        .context(Resources::Remaining, "Rolearn is not a string.")
        .map(|arn| arn.to_string())
}

fn write_kubeconfig(cluster_name: &str, region: &str, kubeconfig_dir: &Path) -> ProviderResult<()> {
    info!("Writing kubeconfig file");
    let status = Command::new("eksctl")
        .args([
            "utils",
            "write-kubeconfig",
            "-r",
            region,
            &format!("--cluster={}", cluster_name),
            &format!(
                "--kubeconfig={}",
                kubeconfig_dir.to_str().context(
                    Resources::Remaining,
                    format!("Unable to convert '{:?}' to string path", kubeconfig_dir),
                )?
            ),
        ])
        .status()
        .context(Resources::Remaining, "Failed write kubeconfig")?;

    if !status.success() {
        return Err(ProviderError::new_with_context(
            Resources::Remaining,
            format!("Failed write kubeconfig with status code {}", status),
        ));
    }

    Ok(())
}

async fn created_cluster(
    encoded_kubeconfig: String,
    cluster_name: &str,
    region: &str,
    aws_clients: &AwsClients,
) -> ProviderResult<CreatedCluster> {
    let eks_subnet_ids = eks_subnet_ids(&aws_clients.eks_client, cluster_name).await?;
    let endpoint = endpoint(&aws_clients.eks_client, cluster_name).await?;
    let certificate = certificate(&aws_clients.eks_client, cluster_name).await?;

    info!("Getting public subnet ids");
    let public_subnet_ids: Vec<String> = subnet_ids(
        &aws_clients.ec2_client,
        cluster_name,
        eks_subnet_ids.clone(),
        SubnetType::Public,
    )
    .await?
    .into_iter()
    .filter_map(|subnet| subnet.subnet_id)
    .collect();
    debug!("Public subnet ids: {:?}", public_subnet_ids);

    info!("Getting private subnet ids");
    let private_subnet_ids: Vec<String> = subnet_ids(
        &aws_clients.ec2_client,
        cluster_name,
        eks_subnet_ids.clone(),
        SubnetType::Private,
    )
    .await?
    .into_iter()
    .filter_map(|subnet| subnet.subnet_id)
    .collect();
    debug!("Private subnet ids: {:?}", private_subnet_ids);

    info!("Getting the nodegroup security group");
    let nodegroup_sg: Vec<String> = security_group(
        &aws_clients.ec2_client,
        cluster_name,
        SecurityGroupType::NodeGroup,
    )
    .await?
    .into_iter()
    .filter_map(|security_group| security_group.group_id)
    .collect();
    debug!("nodegroup_sg: {:?}", nodegroup_sg);

    info!("Getting the controlplane security group");
    let controlplane_sg = security_group(
        &aws_clients.ec2_client,
        cluster_name,
        SecurityGroupType::ControlPlane,
    )
    .await?
    .into_iter()
    .filter_map(|security_group| security_group.group_id)
    .collect();
    debug!("controlplane_sg: {:?}", controlplane_sg);

    info!("Getting the cluster shared security group");
    let clustershared_sg: Vec<String> = security_group(
        &aws_clients.ec2_client,
        cluster_name,
        SecurityGroupType::ClusterShared,
    )
    .await?
    .into_iter()
    .filter_map(|security_group| security_group.group_id)
    .collect();
    debug!("clustershared_sg: {:?}", clustershared_sg);

    let mut security_groups = vec![];
    security_groups.append(&mut nodegroup_sg.clone());
    security_groups.append(&mut clustershared_sg.clone());
    debug!("security_groups: {:?}", security_groups);

    let node_instance_role = cluster_iam_identity_mapping(cluster_name, region)?;
    let iam_instance_profile_arn =
        instance_profile(&aws_clients.iam_client, cluster_name, &node_instance_role).await?;

    Ok(CreatedCluster {
        cluster_name: cluster_name.to_string(),
        region: region.to_string(),
        endpoint,
        certificate,
        public_subnet_id: first_subnet_id(&public_subnet_ids),
        private_subnet_id: first_subnet_id(&private_subnet_ids),
        public_subnet_ids,
        private_subnet_ids,
        nodegroup_sg,
        controlplane_sg,
        clustershared_sg,
        iam_instance_profile_arn,
        security_groups,
        encoded_kubeconfig,
    })
}

async fn eks_subnet_ids(
    eks_client: &aws_sdk_eks::Client,
    cluster_name: &str,
) -> ProviderResult<Vec<String>> {
    let describe_results = eks_client
        .describe_cluster()
        .name(cluster_name)
        .send()
        .await
        .context(Resources::Remaining, "Unable to get eks describe cluster")?;

    // Extract the subnet ids from the cluster.
    describe_results
        .cluster
        .as_ref()
        .context(Resources::Remaining, "Response missing cluster field")?
        .resources_vpc_config
        .as_ref()
        .context(
            Resources::Remaining,
            "Cluster missing resources_vpc_config field",
        )?
        .subnet_ids
        .as_ref()
        .context(
            Resources::Remaining,
            "resources_vpc_config missing subnet ids",
        )
        .map(|ids| ids.clone())
}

async fn endpoint(eks_client: &aws_sdk_eks::Client, cluster_name: &str) -> ProviderResult<String> {
    let describe_results = eks_client
        .describe_cluster()
        .name(cluster_name)
        .send()
        .await
        .context(Resources::Remaining, "Unable to get eks describe cluster")?;
    // Extract the apiserver endpoint from the cluster.
    describe_results
        .cluster
        .as_ref()
        .context(Resources::Remaining, "Results missing cluster field")?
        .endpoint
        .as_ref()
        .context(Resources::Remaining, "Cluster missing endpoint field")
        .map(|ids| ids.clone())
}

async fn certificate(
    eks_client: &aws_sdk_eks::Client,
    cluster_name: &str,
) -> ProviderResult<String> {
    let describe_results = eks_client
        .describe_cluster()
        .name(cluster_name)
        .send()
        .await
        .context(Resources::Remaining, "Unable to get eks describe cluster")?;

    // Extract the certificate authority from the cluster.
    describe_results
        .cluster
        .as_ref()
        .context(Resources::Remaining, "Results missing cluster field")?
        .certificate_authority
        .as_ref()
        .context(
            Resources::Remaining,
            "Cluster missing certificate_authority field",
        )?
        .data
        .as_ref()
        .context(Resources::Remaining, "Certificate authority missing data")
        .map(|ids| ids.clone())
}

fn first_subnet_id(subnet_ids: &[String]) -> Option<String> {
    subnet_ids.get(0).map(|id| id.to_string())
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum SubnetType {
    Public,
    Private,
}

impl SubnetType {
    fn tag(&self, cluster_name: &str) -> String {
        let subnet_type = match self {
            SubnetType::Public => "Public",
            SubnetType::Private => "Private",
        };
        format!("eksctl-{}-cluster/Subnet{}*", cluster_name, subnet_type)
    }
}

async fn subnet_ids(
    ec2_client: &aws_sdk_ec2::Client,
    cluster_name: &str,
    eks_subnet_ids: Vec<String>,
    subnet_type: SubnetType,
) -> ProviderResult<Vec<Subnet>> {
    let describe_results = ec2_client
        .describe_subnets()
        .set_subnet_ids(Some(eks_subnet_ids))
        .filters(
            Filter::builder()
                .name("tag:Name")
                .values(subnet_type.tag(cluster_name))
                .build(),
        )
        .send()
        .await
        .context(Resources::Remaining, "Unable to get private subnet ids")?;
    describe_results
        .subnets
        .context(Resources::Remaining, "Results missing subnets field")
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum SecurityGroupType {
    NodeGroup,
    ClusterShared,
    ControlPlane,
}

impl SecurityGroupType {
    fn tag(&self, cluster_name: &str) -> String {
        let sg = match self {
            SecurityGroupType::NodeGroup => "nodegroup",
            SecurityGroupType::ClusterShared => "cluster/ClusterSharedNodeSecurityGroup",
            SecurityGroupType::ControlPlane => "cluster/ControlPlaneSecurityGroup",
        };
        format!("*{}-{}*", cluster_name, sg)
    }
}

async fn security_group(
    ec2_client: &aws_sdk_ec2::Client,
    cluster_name: &str,
    security_group_type: SecurityGroupType,
) -> ProviderResult<Vec<SecurityGroup>> {
    // Extract the security groups.
    let filter_value = security_group_type.tag(cluster_name);
    trace!("Filtering for tag:Name={}", filter_value);
    let describe_results = ec2_client
        .describe_security_groups()
        .filters(
            Filter::builder()
                .name("tag:Name")
                .values(filter_value.clone())
                .build(),
        )
        .send()
        .await
        .context(
            Resources::Remaining,
            format!("Unable to get {:?} security group", security_group_type),
        )?;

    let security_groups = describe_results.security_groups.context(
        Resources::Remaining,
        "Results missing security_groups field",
    )?;

    // If we haven't found the security group (or we found too many), the user may experience hard-
    // to-diagnose issues downstream, so we want to raise an error here.
    if security_groups.is_empty() {
        return Err(ProviderError::new_with_context(
            Resources::Remaining,
            format!(
                "Security group not found when filtering the name tags with '{}'",
                filter_value
            ),
        ));
    } else if security_groups.len() > 1 {
        return Err(ProviderError::new_with_context(
            Resources::Remaining,
            format!(
                "More than one security group found when filtering the name tags with '{}'",
                filter_value
            ),
        ));
    }

    Ok(security_groups)
}

async fn instance_profile(
    iam_client: &aws_sdk_iam::Client,
    cluster_name: &str,
    node_instance_role: &str,
) -> ProviderResult<String> {
    let list_result = iam_client
        .list_instance_profiles()
        .send()
        .await
        .context(Resources::Remaining, "Unable to list instance profiles")?;
    let eksctl_prefix = format!("eksctl-{}", cluster_name);
    list_result
        .instance_profiles
        .as_ref()
        .context(Resources::Remaining, "No instance profiles found")?
        .iter()
        .filter(|x| {
            x.instance_profile_name
                .as_ref()
                .unwrap_or(&"".to_string())
                .contains("NodeInstanceProfile")
        })
        .filter(|x| {
            x.instance_profile_name
                .as_ref()
                .unwrap_or(&"".to_string())
                .contains(&eksctl_prefix)
        })
        .find(|instance_profile| {
            instance_profile
                .roles
                .as_ref()
                .map(|roles| {
                    roles
                        .iter()
                        .any(|role| role.arn == Some(node_instance_role.to_string()))
                })
                .unwrap_or_default()
        })
        .context(Resources::Remaining, "Node instance profile not found")?
        .arn
        .as_ref()
        .context(
            Resources::Remaining,
            "Node instance profile missing arn field",
        )
        .map(|profile| profile.clone())
}

async fn does_cluster_exist(name: &str, aws_clients: &AwsClients) -> ProviderResult<bool> {
    let describe_cluster_result = aws_clients
        .eks_client
        .describe_cluster()
        .name(name)
        .send()
        .await;
    if not_found(&describe_cluster_result) {
        return Ok(false);
    }
    let _ = describe_cluster_result.context(
        Resources::Clear,
        format!("Unable to determine if cluster '{}' exists", name),
    )?;
    Ok(true)
}

fn not_found(
    result: &std::result::Result<DescribeClusterOutput, SdkError<DescribeClusterError>>,
) -> bool {
    if let Err(SdkError::ServiceError { err, raw: _ }) = result {
        if matches!(
            &err.kind,
            DescribeClusterErrorKind::ResourceNotFoundException(_)
        ) {
            return true;
        }
    }
    false
}

pub struct EksDestroyer {}

#[async_trait::async_trait]
impl Destroy for EksDestroyer {
    type Config = ClusterConfig;
    type Info = ProductionMemo;
    type Resource = CreatedCluster;

    async fn destroy<I>(
        &self,
        _: Option<Spec<Self::Config>>,
        _: Option<Self::Resource>,
        client: &I,
    ) -> ProviderResult<()>
    where
        I: InfoClient,
    {
        let mut memo: ProductionMemo = client
            .get_info()
            .await
            .context(Resources::Remaining, "Unable to get info from client")?;

        let cluster_name = match &memo.cluster_name {
            Some(x) => x,
            None => {
                return Err(ProviderError::new_with_context(
                    Resources::Unknown,
                    "Unable to obtain cluster name",
                ))
            }
        };

        // Write aws credentials if we need them so we can run eksctl
        if let Some(aws_secret_name) = &memo.aws_secret_name {
            setup_env(client, aws_secret_name, Resources::Remaining).await?;
        }
        let region = memo
            .clone()
            .region
            .unwrap_or_else(|| DEFAULT_REGION.to_string());

        let status = Command::new("eksctl")
            .args(["delete", "cluster", "--name", cluster_name, "-r", &region])
            .status()
            .context(Resources::Remaining, "Failed to run eksctl delete command")?;
        if !status.success() {
            return Err(ProviderError::new_with_context(
                Resources::Orphaned,
                format!("Failed to delete cluster with status code {}", status),
            ));
        }

        memo.current_status = "Cluster deleted".into();
        if let Err(e) = client.send_info(memo.clone()).await {
            eprintln!(
                "Cluster deleted but failed to send info message to k8s: {}",
                e
            )
        }

        Ok(())
    }
}
