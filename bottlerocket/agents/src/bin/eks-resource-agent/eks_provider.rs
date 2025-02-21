use agent_utils::aws::aws_config;
use agent_utils::{impl_display_as_json, json_display};
use aws_sdk_cloudformation::types::StackStatus;
use aws_sdk_ec2::types::{Filter, Subnet};
use aws_sdk_eks::error::SdkError as EksSdkError;
use aws_sdk_eks::operation::describe_cluster::{DescribeClusterError, DescribeClusterOutput};
use aws_sdk_eks::types::{Cluster, IpFamily};
use aws_types::SdkConfig;
use base64::engine::general_purpose::STANDARD as Base64;
use base64::Engine;
use bottlerocket_agents::is_cluster_creation_required;
use bottlerocket_types::agent_config::{
    CreationPolicy, EksClusterConfig, EksctlConfig, K8sVersion, AWS_CREDENTIALS_SECRET_NAME,
};
use log::{debug, info, trace};
use resource_agent::clients::InfoClient;
use resource_agent::provider::{
    Create, Destroy, IntoProviderError, ProviderError, ProviderResult, Resources, Spec,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env::temp_dir;
use std::path::Path;
use std::process::Command;
use testsys_model::{Configuration, SecretName};

use std::fs::File;
use std::io::Write;
use strum_macros::EnumString;

/// The default region for the cluster.
const DEFAULT_REGION: &str = "us-west-2";
/// The default cluster version.
const DEFAULT_VERSION: &str = "1.32";
const TEST_CLUSTER_CONFIG_PATH: &str = "/local/eksctl_config.yaml";
const CLUSTER_CONFIG_PATH: &str = "/local/cluster_config.yaml";
/// The default cluster managed node group values.
const MNG_MIN_SIZE: i32 = 0;
const MNG_MAX_SIZE: i32 = 2;
const MNG_DESIRED_CAPACITY: i32 = 0;

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

    /// The region the cluster is in.
    pub region: Option<String>,

    /// The role arn that is being assumed.
    pub assume_role: Option<String>,

    pub provisioning_started: bool,
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

    /// The cluster DNS IP.
    pub cluster_dns_ip: String,

    /// A vector of public subnet ids. Will be empty if no public ids exist.
    pub public_subnet_ids: Vec<String>,

    /// A vector of private subnet ids. Will be empty if no private ids exist.
    pub private_subnet_ids: Vec<String>,

    /// Security groups necessary for ec2 instances
    pub security_groups: Vec<String>,

    /// Cluster security group
    pub cluster_sg: String,

    /// The instance IAM instance profile.
    pub iam_instance_profile_arn: String,

    /// Base64 encoded kubeconfig
    pub encoded_kubeconfig: String,
}

impl Configuration for CreatedCluster {}
impl_display_as_json!(CreatedCluster);

#[derive(Debug)]
struct AwsClients {
    eks_client: aws_sdk_eks::Client,
    ec2_client: aws_sdk_ec2::Client,
    iam_client: aws_sdk_iam::Client,
    cfn_client: aws_sdk_cloudformation::Client,
}

impl AwsClients {
    async fn new(shared_config: &SdkConfig, eks_config: &SdkConfig) -> Self {
        Self {
            eks_client: aws_sdk_eks::Client::new(eks_config),
            ec2_client: aws_sdk_ec2::Client::new(shared_config),
            iam_client: aws_sdk_iam::Client::new(shared_config),
            cfn_client: aws_sdk_cloudformation::Client::new(shared_config),
        }
    }
}
enum ClusterConfig {
    Args {
        cluster_name: String,
        region: String,
        version: Option<K8sVersion>,
        zones: Option<Vec<String>>,
    },
    ConfigPath {
        cluster_name: String,
        region: String,
    },
}

#[derive(Serialize, Debug, EnumString)]
enum IPFamily {
    IPv6,
    IPv4,
}

/// Configuration for setting up an EKS cluster using eksctl yaml file.
///
/// # Fields:
/// - `api_version`: Specifies the version of the EKS configuration API.
/// - `kind`: Indicates the type of the configuration, typically "ClusterConfig".
/// - `metadata`: Metadata about the cluster for instance name, region, and version.
/// - `availability_zones`: List of availability zones where cluster should be deployed.
/// - `kubernetes_network_config`: Configuration for the Kubernetes network: IPv6 or IPv4.
/// - `addons`: List of EKS addons to be enabled on the cluster.
/// - `iam`: IAM configuration, especially for OIDC.
/// - `managed_node_groups`: List of managed node groups for the cluster.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EksctlYamlConfig {
    /// Version of the EKS configuration API
    api_version: String,
    /// Type of the configuration, typically "ClusterConfig".
    kind: String,
    /// Metadata about the cluster for instance name, region, and version.
    #[serde(rename = "metadata")]
    eksctl_metadata: EksctlMetadata,
    /// List of availability zones where cluster should be deployed.
    availability_zones: Vec<String>,
    /// Configuration for the Kubernetes network: IPv6 or IPv4.
    kubernetes_network_config: KubernetesNetworkConfig,
    /// List of EKS addons to be enabled on the cluster.
    addons: Vec<Addon>,
    /// IAM configuration, especially for OIDC.
    iam: IAMConfig,
    /// List of managed node groups for the cluster.
    managed_node_groups: Vec<ManagedNodeGroup>,
}

/// Metadata for configuration of the EKS cluster.
///
/// # Fields:
/// - `name`: The name of the cluster.
/// - `region`: AWS region where the cluster will be deployed.
/// - `version`: The Kubernetes version will be used.
#[derive(Serialize)]
struct EksctlMetadata {
    /// Name of the cluster
    name: String,
    /// AWS region where the cluster will be deployed.
    region: String,
    /// The Kubernetes version will be used.
    version: String,
}

/// Kubernetes network setup.
///
/// # Fields:
/// - `ip_family`: Specifies whether IPv4 or IPv6.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KubernetesNetworkConfig {
    /// Specifies whether IPv4 or IPv6.
    ip_family: IPFamily,
}

/// Addon that can be configured for the EKS cluster.
///
/// # Fields:
/// - `name`: The name of the addon.
/// - `version`: The version of the addon.
#[derive(Serialize)]
struct Addon {
    /// Name of the addon.
    name: String,
    /// Version of the addon.
    version: String,
}

/// IAM configuration.
///
/// # Fields:
/// - `withOIDC`: Flag to enable OIDC.
#[derive(Serialize)]
#[allow(non_snake_case)]
struct IAMConfig {
    /// Flag to enable OIDC
    withOIDC: bool,
}

/// Managed node group in the EKS cluster.
///
/// # Fields:
/// - `name`: The name of the managed node group.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ManagedNodeGroup {
    /// Name of the managed node group
    name: String,
    /// The minimum number of nodes in the managed node group.
    min_size: i32,
    /// The maximum number of nodes in the managed node group.
    max_size: i32,
    // The desired number of nodes in the managed node group.
    desired_capacity: i32,
}

#[allow(clippy::unwrap_or_default)]
fn create_yaml(
    cluster_name: &str,
    region: &str,
    version: &str,
    zones: &Option<Vec<String>>,
) -> ProviderResult<()> {
    let set_ip_family = if cluster_name.ends_with("ipv6") {
        IPFamily::IPv6
    } else {
        IPFamily::IPv4
    };

    let cluster = EksctlYamlConfig {
        api_version: "eksctl.io/v1alpha5".to_string(),
        kind: "ClusterConfig".to_string(),
        eksctl_metadata: EksctlMetadata {
            name: cluster_name.to_string(),
            region: region.to_string(),
            version: version.to_string(),
        },
        availability_zones: zones.clone().unwrap_or_else(Vec::new),
        kubernetes_network_config: KubernetesNetworkConfig {
            ip_family: set_ip_family,
        },
        addons: vec![
            Addon {
                name: "vpc-cni".to_string(),
                version: "latest".to_string(),
            },
            Addon {
                name: "coredns".to_string(),
                version: "latest".to_string(),
            },
            Addon {
                name: "kube-proxy".to_string(),
                version: "latest".to_string(),
            },
        ],
        iam: IAMConfig { withOIDC: true },
        managed_node_groups: vec![ManagedNodeGroup {
            name: "mng-1".to_string(),
            min_size: MNG_MIN_SIZE,
            max_size: MNG_MAX_SIZE,
            desired_capacity: MNG_DESIRED_CAPACITY,
        }],
    };

    let yaml =
        serde_yaml::to_string(&cluster).context(Resources::Clear, "Failed to serialize YAML")?;

    let mut file = File::create(CLUSTER_CONFIG_PATH)
        .context(Resources::Clear, "Failed to create yaml file")?;

    file.write_all(yaml.as_bytes()).context(
        Resources::Clear,
        format!(
            "Unable to write eksctl configuration to '{}'",
            CLUSTER_CONFIG_PATH
        ),
    )?;
    info!("YAML file has been created at CLUSTER_CONFIG_PATH");

    Ok(())
}

impl ClusterConfig {
    pub fn new(eksctl_config: EksctlConfig) -> ProviderResult<Self> {
        let config = match eksctl_config {
            EksctlConfig::File { encoded_config } => {
                let decoded_config = Base64
                    .decode(encoded_config)
                    .context(Resources::Clear, "Unable to decode eksctl configuration.")?;

                let config: Value =
                    serde_yaml::from_str(std::str::from_utf8(&decoded_config).context(
                        Resources::Clear,
                        "Unable to convert decoded config to string.",
                    )?)
                    .context(Resources::Clear, "Unable to serialize eksctl config.")?;

                let (cluster_name, region) = config
                    .get("metadata")
                    .map(|metadata| {
                        (
                            metadata.get("name").and_then(|name| name.as_str()),
                            metadata.get("region").and_then(|region| region.as_str()),
                        )
                    })
                    .context(Resources::Clear, "Metadata is missing from eksctl config.")?;

                Self::ConfigPath {
                    cluster_name: cluster_name
                        .context(
                            Resources::Clear,
                            "The cluster's name was not in the eksctl config.",
                        )?
                        .to_string(),
                    region: region
                        .context(
                            Resources::Clear,
                            "The cluster's region was not in the eksctl config.",
                        )?
                        .to_string(),
                }
            }
            EksctlConfig::Args {
                cluster_name,
                region,
                zones,
                version,
            } => Self::Args {
                cluster_name,
                region: region.unwrap_or_else(|| DEFAULT_REGION.to_string()),
                version,
                zones,
            },
        };
        Ok(config)
    }

    /// Create a cluster with the given config.
    pub fn create_cluster(&self) -> ProviderResult<()> {
        let cluster_config_path = match self {
            Self::Args {
                cluster_name,
                region,
                version,
                zones,
            } => {
                let version_arg = version
                    .as_ref()
                    .map(|version| version.major_minor_without_v())
                    .unwrap_or_else(|| DEFAULT_VERSION.to_string());

                create_yaml(cluster_name, region, &version_arg, zones)?;
                trace!(
                    "assigned create cluster yaml file path is {}",
                    CLUSTER_CONFIG_PATH
                );
                CLUSTER_CONFIG_PATH
            }
            Self::ConfigPath {
                cluster_name: _,
                region: _,
            } => {
                trace!(
                    "assigned create cluster yaml file path is {}",
                    TEST_CLUSTER_CONFIG_PATH
                );
                TEST_CLUSTER_CONFIG_PATH
            }
        };

        trace!("Calling eksctl create cluster with config file");
        let status = Command::new("eksctl")
            .args(["create", "cluster", "-f", cluster_config_path])
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

    pub fn region(&self) -> String {
        match self {
            Self::Args {
                cluster_name: _,
                region,
                version: _,
                zones: _,
            } => region.to_string(),
            Self::ConfigPath {
                cluster_name: _,
                region,
            } => region.to_string(),
        }
    }

    pub fn cluster_name(&self) -> String {
        match self {
            Self::Args {
                cluster_name,
                region: _,
                version: _,
                zones: _,
            } => cluster_name.to_string(),
            Self::ConfigPath {
                cluster_name,
                region: _,
            } => cluster_name.to_string(),
        }
    }
}

pub struct EksCreator {}

#[async_trait::async_trait]
impl Create for EksCreator {
    type Config = EksClusterConfig;
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
        info!("Initializing agent");
        memo.current_status = "Initializing agent".to_string();
        memo.creation_policy = Some(spec.configuration.creation_policy.unwrap_or_default());
        client
            .send_info(memo.clone())
            .await
            .context(Resources::Clear, "Error sending cluster creation message")?;

        let cluster_config = ClusterConfig::new(spec.configuration.config)?;

        info!(
            "Beginning creation of EKS cluster '{}' with creation policy '{:?}'",
            cluster_config.cluster_name(),
            memo.creation_policy
        );

        info!("Getting AWS secret");
        memo.current_status = "Getting AWS secret".to_string();
        client
            .send_info(memo.clone())
            .await
            .context(Resources::Clear, "Error sending cluster creation message")?;

        memo.aws_secret_name = spec.secrets.get(AWS_CREDENTIALS_SECRET_NAME).cloned();
        memo.assume_role.clone_from(&spec.configuration.assume_role);

        info!("Creating AWS config");
        memo.current_status = "Creating AWS config".to_string();
        client
            .send_info(memo.clone())
            .await
            .context(Resources::Clear, "Error sending cluster creation message")?;

        let shared_config = aws_config(
            &spec.secrets.get(AWS_CREDENTIALS_SECRET_NAME),
            &spec.configuration.assume_role,
            &None,
            &Some(cluster_config.region()),
            &None,
            true,
        )
        .await
        .context(Resources::Clear, "Error creating config")?;

        let eks_sdk_config = aws_config(
            &spec.secrets.get(AWS_CREDENTIALS_SECRET_NAME),
            &spec.configuration.assume_role,
            &None,
            &Some(cluster_config.region()),
            &spec.configuration.eks_service_endpoint,
            true,
        )
        .await
        .context(Resources::Clear, "Error creating EKS client config")?;

        let aws_clients = AwsClients::new(&shared_config, &eks_sdk_config).await;

        info!("Determining cluster state");
        memo.current_status = "Determining cluster state".to_string();
        client
            .send_info(memo.clone())
            .await
            .context(Resources::Clear, "Error sending cluster creation message")?;

        let (do_create, message) = is_eks_cluster_creation_required(
            &cluster_config.cluster_name(),
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
            memo.current_status = "Creating cluster".to_string();
            memo.provisioning_started = true;
            client
                .send_info(memo.clone())
                .await
                .context(Resources::Clear, "Error sending cluster creation message")?;
            cluster_config.create_cluster()?;
            info!("Done creating cluster with eksctl");
            memo.current_status = "Cluster creation complete".to_string();
            client.send_info(memo.clone()).await.context(
                Resources::Remaining,
                "Error sending cluster creation message",
            )?;
        }

        info!("Writing cluster kubeconfig");
        memo.current_status = "Writing cluster kubeconfig".to_string();
        client.send_info(memo.clone()).await.context(
            Resources::Remaining,
            "Error sending cluster creation message",
        )?;

        write_kubeconfig(
            &cluster_config.cluster_name(),
            &spec.configuration.eks_service_endpoint,
            &cluster_config.region(),
            &kubeconfig_dir,
        )?;
        let kubeconfig = std::fs::read_to_string(kubeconfig_dir)
            .context(Resources::Remaining, "Unable to read kubeconfig.")?;
        let encoded_kubeconfig = Base64.encode(kubeconfig);

        info!("Gathering information about the cluster");

        memo.current_status = "Collecting cluster info".to_string();
        client.send_info(memo.clone()).await.context(
            Resources::Remaining,
            "Error sending cluster creation message",
        )?;

        let created_cluster = created_cluster(
            encoded_kubeconfig,
            &cluster_config.cluster_name(),
            &cluster_config.region(),
            &aws_clients,
        )
        .await?;

        memo.current_status = "Cluster ready".into();
        memo.cluster_name = Some(cluster_config.cluster_name());
        memo.region = Some(cluster_config.region());
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

/// Returns the bool telling us whether or not we need to create the cluster, and a string that
/// explains why that we can use for logging or memo status. Returns an error if the creation policy
/// requires us to create a cluster when it already exists, or creation policy forbids us to create
/// a cluster and it does not exist.
async fn is_eks_cluster_creation_required(
    cluster_name: &str,
    creation_policy: CreationPolicy,
    aws_clients: &AwsClients,
) -> ProviderResult<(bool, String)> {
    let cluster_exists: bool = does_cluster_exist(cluster_name, aws_clients).await?;
    is_cluster_creation_required(&cluster_exists, cluster_name, &creation_policy).await
}

async fn nodegroup_iam_role(
    cluster_name: &str,
    cfn_client: &aws_sdk_cloudformation::Client,
) -> ProviderResult<String> {
    let mut list_stack_output = cfn_client
        .list_stacks()
        .stack_status_filter(StackStatus::CreateComplete)
        .stack_status_filter(StackStatus::UpdateComplete)
        .send()
        .await
        .context(Resources::Remaining, "Unable to list CloudFormation stacks")?;

    let stack_name;
    loop {
        if let Some(name) = list_stack_output
            .stack_summaries()
            .iter()
            .filter_map(|stack| stack.stack_name())
            .find(|name|
                // For eksctl created clusters
                name.starts_with(&format!("eksctl-{cluster_name}-nodegroup"))
                    // For non-eksctl created clusters
                    || name.starts_with(&format!("{cluster_name}-node-group")))
        {
            stack_name = name;
            break;
        } else if let Some(token) = list_stack_output.next_token() {
            list_stack_output = cfn_client
                .list_stacks()
                .next_token(token)
                .stack_status_filter(StackStatus::CreateComplete)
                .stack_status_filter(StackStatus::UpdateComplete)
                .send()
                .await
                .context(Resources::Remaining, "Unable to list CloudFormation stacks")?;
            continue;
        } else {
            return Err(ProviderError::new_with_context(
                Resources::Remaining,
                "Could not find nodegroup cloudformation stack for cluster",
            ));
        }
    }

    cfn_client
        .describe_stack_resource()
        .stack_name(stack_name)
        .logical_resource_id("NodeInstanceRole")
        .send()
        .await
        .context(
            Resources::Remaining,
            format!("Unable to describe CloudFormation stack resources for '{stack_name}'"),
        )?
        .stack_resource_detail()
        .context(
            Resources::Remaining,
            format!("Missing 'NodeInstanceRole' stack resource for '{stack_name}'"),
        )?
        .physical_resource_id()
        .context(
            Resources::Remaining,
            format!("Missing stack outputs in '{stack_name}'"),
        )
        .map(|s| s.to_string())
}

fn write_kubeconfig(
    cluster_name: &str,
    endpoint: &Option<String>,
    region: &str,
    kubeconfig_dir: &Path,
) -> ProviderResult<()> {
    info!("Updating kubeconfig file");
    let mut aws_cli_args = vec![
        "eks",
        "update-kubeconfig",
        "--region",
        region,
        "--name",
        cluster_name,
        "--kubeconfig",
        kubeconfig_dir.to_str().context(
            Resources::Remaining,
            format!("Unable to convert '{:?}' to string path", kubeconfig_dir),
        )?,
    ];
    if let Some(endpoint) = endpoint {
        info!("Using EKS service endpoint: {}", endpoint);
        aws_cli_args.append(&mut vec!["--endpoint", endpoint]);
    }
    let status = Command::new("aws")
        .args(aws_cli_args)
        .status()
        .context(Resources::Remaining, "Failed update kubeconfig")?;

    if !status.success() {
        return Err(ProviderError::new_with_context(
            Resources::Remaining,
            format!("Failed update kubeconfig with status code {}", status),
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
    let cluster = aws_clients
        .eks_client
        .describe_cluster()
        .name(cluster_name)
        .send()
        .await
        .context(Resources::Remaining, "Unable to get eks describe cluster")?
        .cluster
        .context(Resources::Remaining, "Response missing cluster field")?;
    let eks_subnet_ids = eks_subnet_ids(&cluster).await?;
    let endpoint = endpoint(&cluster).await?;
    let certificate = certificate(&cluster).await?;
    let cluster_dns_ip = cluster_dns_ip(&cluster).await?;
    let cluster_sg = cluster_sg(&cluster).await?;

    info!("Getting public subnet ids");
    let public_subnet_ids: Vec<String> = subnet_ids(
        &aws_clients.ec2_client,
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
        eks_subnet_ids.clone(),
        SubnetType::Private,
    )
    .await?
    .into_iter()
    .filter_map(|subnet| subnet.subnet_id)
    .collect();
    debug!("Private subnet ids: {:?}", private_subnet_ids);

    let security_groups = vec![cluster_sg.to_owned()];
    debug!("security_groups: {:?}", security_groups);

    let node_instance_role = nodegroup_iam_role(cluster_name, &aws_clients.cfn_client).await?;
    let iam_instance_profile_arn =
        instance_profile_arn(&aws_clients.iam_client, &node_instance_role).await?;

    Ok(CreatedCluster {
        cluster_name: cluster_name.to_string(),
        region: region.to_string(),
        endpoint,
        certificate,
        cluster_dns_ip,
        public_subnet_ids,
        private_subnet_ids,
        cluster_sg,
        iam_instance_profile_arn,
        security_groups,
        encoded_kubeconfig,
    })
}

async fn eks_subnet_ids(cluster: &Cluster) -> ProviderResult<Vec<String>> {
    cluster
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
        .cloned()
}

async fn cluster_sg(cluster: &Cluster) -> ProviderResult<String> {
    cluster
        .resources_vpc_config
        .as_ref()
        .context(
            Resources::Remaining,
            "Cluster missing resources_vpc_config field",
        )?
        .cluster_security_group_id()
        .context(
            Resources::Remaining,
            "resources_vpc_config missing cluster security group id",
        )
        .map(|s| s.to_string())
}

async fn endpoint(cluster: &Cluster) -> ProviderResult<String> {
    cluster
        .endpoint
        .as_ref()
        .context(Resources::Remaining, "Cluster missing endpoint field")
        .cloned()
}

async fn certificate(cluster: &Cluster) -> ProviderResult<String> {
    cluster
        .certificate_authority
        .as_ref()
        .context(
            Resources::Remaining,
            "Cluster missing certificate_authority field",
        )?
        .data
        .as_ref()
        .context(Resources::Remaining, "Certificate authority missing data")
        .cloned()
}

async fn cluster_dns_ip(cluster: &Cluster) -> ProviderResult<String> {
    let kubernetes_network_config = cluster.kubernetes_network_config.as_ref().context(
        Resources::Remaining,
        "DescribeCluster missing KubernetesNetworkConfig field",
    )?;

    service_ip_cidr_to_cluster_dns_ip(
        kubernetes_network_config.ip_family.clone(),
        kubernetes_network_config.service_ipv4_cidr.clone(),
        kubernetes_network_config.service_ipv6_cidr.clone(),
    )
}

fn service_ip_cidr_to_cluster_dns_ip(
    ip_family: Option<IpFamily>,
    service_ipv4_cidr: Option<String>,
    service_ipv6_cidr: Option<String>,
) -> ProviderResult<String> {
    let ip_family = ip_family.as_ref().context(
        Resources::Remaining,
        "Cluster network config missing IP family information",
    )?;
    match ip_family {
        IpFamily::Ipv6 => {
            let service_ipv6_cidr = service_ipv6_cidr.as_ref().context(
                Resources::Remaining,
                "IPv6 Cluster missing serviceIPv6CIDR information",
            )?;
            Ok(service_ipv6_cidr.split('/').collect::<Vec<&str>>()[0].to_string() + "a")
        }
        // If IP family is IPv4 or unknown, we fallback to deriving the cluster dns IP from service IPv4 CIDR
        _ => {
            let service_ipv4_cidr = service_ipv4_cidr.as_ref().context(
                Resources::Remaining,
                "IPv4 Cluster missing serviceIPv4CIDR information",
            )?;
            let mut cidr_split: Vec<&str> = service_ipv4_cidr.split('.').collect();
            if cidr_split.len() != 4 {
                return Err(ProviderError::new_with_context(
                    Resources::Remaining,
                    format!(
                        "Expected 4 components in serviceIPv4CIDR but found {}",
                        cidr_split.len()
                    ),
                ));
            }
            cidr_split[3] = "10";
            Ok(cidr_split.join("."))
        }
    }
}

#[test]
fn cluster_dns_ip_from_service_ipv4_cidr() {
    assert_eq!(
        service_ip_cidr_to_cluster_dns_ip(
            Some(IpFamily::Ipv4),
            Some("10.100.0.0/16".to_string()),
            None
        )
        .unwrap(),
        "10.100.0.10".to_string()
    )
}

#[test]
fn cluster_dns_ip_from_service_ipv6_cidr() {
    assert_eq!(
        service_ip_cidr_to_cluster_dns_ip(
            Some(IpFamily::Ipv6),
            None,
            Some("fd30:1c53:5f8a::/108".to_string())
        )
        .unwrap(),
        "fd30:1c53:5f8a::a".to_string()
    )
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum SubnetType {
    Public,
    Private,
}

async fn subnet_ids(
    ec2_client: &aws_sdk_ec2::Client,
    eks_subnet_ids: Vec<String>,
    subnet_type: SubnetType,
) -> ProviderResult<Vec<Subnet>> {
    let describe_results = ec2_client
        .describe_subnets()
        .set_subnet_ids(Some(eks_subnet_ids))
        .filters(
            Filter::builder()
                .name("map-public-ip-on-launch")
                .values(match subnet_type {
                    SubnetType::Public => "true",
                    SubnetType::Private => "false",
                })
                .build(),
        )
        .send()
        .await
        .context(Resources::Remaining, "Unable to get private subnet ids")?;
    describe_results
        .subnets
        .context(Resources::Remaining, "Results missing subnets field")
}

async fn instance_profile_arn(
    iam_client: &aws_sdk_iam::Client,
    role_name: &str,
) -> ProviderResult<String> {
    let instance_profiles = iam_client
        .list_instance_profiles_for_role()
        .role_name(role_name)
        .send()
        .await
        .context(
            Resources::Remaining,
            format!("Unable to list instance profiles for role '{}'", role_name),
        )?
        .instance_profiles;

    if instance_profiles.len() > 1 {
        return Err(ProviderError::new_with_context(
            Resources::Remaining,
            format!(
                "More than one instance profile was found for role '{}'",
                role_name
            ),
        ));
    }

    let instance_profile = instance_profiles.into_iter().next().ok_or_else(|| {
        ProviderError::new_with_context(
            Resources::Remaining,
            format!("No instance profile was found for role '{}'", role_name),
        )
    })?;

    Ok(instance_profile.arn)
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
    result: &std::result::Result<DescribeClusterOutput, EksSdkError<DescribeClusterError>>,
) -> bool {
    if let Err(EksSdkError::ServiceError(service_error)) = result {
        if matches!(
            &service_error.err(),
            DescribeClusterError::ResourceNotFoundException(_)
        ) {
            return true;
        }
    }
    false
}

pub struct EksDestroyer {}

#[async_trait::async_trait]
impl Destroy for EksDestroyer {
    type Config = EksClusterConfig;
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

        if !memo.provisioning_started {
            return Ok(());
        }

        let cluster_name = match &memo.cluster_name {
            Some(cluster_name) => cluster_name,
            None => {
                return Err(ProviderError::new_with_context(
                    Resources::Unknown,
                    "Unable to obtain cluster name",
                ))
            }
        };

        let _ = aws_config(
            &memo.aws_secret_name.as_ref(),
            &memo.assume_role,
            &None,
            &memo.region.clone(),
            &None,
            true,
        )
        .await
        .context(Resources::Clear, "Error creating config")?;

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

        info!("Cluster deleted");
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
