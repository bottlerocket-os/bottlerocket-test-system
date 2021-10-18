use aws_config::meta::region::RegionProviderChain;
use aws_sdk_ec2::model::{
    ArchitectureValues, Filter, IamInstanceProfileSpecification, InstanceType, ResourceType,
    SecurityGroup, Subnet, Tag, TagSpecification,
};
use aws_sdk_ec2::Region;
use model::{Configuration, SecretName};
use resource_agent::clients::InfoClient;
use resource_agent::provider::{
    AsResources, Create, Destroy, IntoProviderError, ProviderError, ProviderResult, Resources, Spec,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::env;
use std::fmt::Debug;
use std::iter::FromIterator;

/// The default region for the cluster.
const DEFAULT_REGION: &str = "us-west-2";
/// The default number of instances to spin up.
const DEFAULT_INSTANCE_COUNT: i32 = 2;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProductionMemo {
    /// In this resource we put some traces here that describe what our provider is doing.
    pub current_status: String,

    /// Ids of all created ec2 instances.
    pub instance_ids: HashSet<String>,

    /// The region the clusters are in.
    pub region: String,

    /// The name of the secret containing aws credentials.
    pub aws_secret_name: Option<SecretName>,
}

impl Configuration for ProductionMemo {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct Ec2ProductionRequest {
    /// The AMI ID of the AMI to use for the worker nodes.
    node_ami: String,

    /// The name of the eks cluster to create the instances in.
    cluster_name: String,

    /// The AWS region to create the instance. If no value is provided `us-west-2` will be used.
    region: Option<String>,

    /// The number of instances to create. If no value is provided 2 instances will be created.
    instance_count: Option<i32>,

    /// The type of instance to spin up. m5.large is recommended for x86_64 and m6g.large is
    /// recommended for arm64. If no value is provided the recommended type will be used.
    instance_type: Option<String>,

    /// The arn of the instance profile.
    instance_profile_name: Option<String>,
}

impl Configuration for Ec2ProductionRequest {}

/// Once we have fulfilled the `Create` request, we return information about the batch of ec2 instances we
/// created.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct CreatedEc2Instances {
    /// The ids of all created instances
    pub ids: HashSet<String>,
}

impl Configuration for CreatedEc2Instances {}

pub struct Ec2Creator {}

#[async_trait::async_trait]
impl Create for Ec2Creator {
    type Info = ProductionMemo;
    type Request = Ec2ProductionRequest;
    type Resource = CreatedEc2Instances;

    async fn create<I>(
        &self,
        request: Spec<Self::Request>,
        client: &I,
    ) -> ProviderResult<Self::Resource>
    where
        I: InfoClient,
    {
        let mut memo: ProductionMemo = client
            .get_info()
            .await
            .context(Resources::Unknown, "Unable to get info from info client")?;
        let region = request
            .configuration
            .region
            .unwrap_or_else(|| DEFAULT_REGION.to_string());
        // Setup aws_sdk_config and clients.
        let region_provider = RegionProviderChain::first_try(Some(Region::new(region.clone())));
        // Write aws credentials if we need them so we can run eksctl
        if let Some(aws_secret_name) = request.secrets.get("aws-credentials") {
            setup_env(client, aws_secret_name, &memo).await?;
            memo.aws_secret_name = Some(aws_secret_name.clone());
        }

        let shared_config = aws_config::from_env().region(region_provider).load().await;
        let eks_client = aws_sdk_eks::Client::new(&shared_config);
        let ec2_client = aws_sdk_ec2::Client::new(&shared_config);
        let iam_client = aws_sdk_iam::Client::new(&shared_config);

        let cluster_name = &request.configuration.cluster_name;

        let eks_subnet_ids = eks_subnet_ids(&eks_client, cluster_name, &memo).await?;
        let endpoint = &endpoint(&eks_client, cluster_name, &memo).await?;
        let certificate = &certificate(&eks_client, cluster_name, &memo).await?;

        let private_subnet_ids = subnet_ids(
            &ec2_client,
            cluster_name,
            eks_subnet_ids.clone(),
            SubnetType::Private,
            &memo,
        )
        .await?;

        let nodegroup_sg = security_group(
            &ec2_client,
            cluster_name,
            SecurityGroupType::NodeGroup,
            &memo,
        )
        .await?;

        let controlplane_sg = security_group(
            &ec2_client,
            cluster_name,
            SecurityGroupType::ControlPlane,
            &memo,
        )
        .await?;

        let mut security_groups = vec![];
        for security_group in nodegroup_sg {
            security_groups.push(
                security_group
                    .group_id
                    .context(&memo, "Security group missing group_id field")?,
            )
        }
        for security_group in controlplane_sg {
            security_groups.push(
                security_group
                    .group_id
                    .context(&memo, "Security group missing group_id field")?,
            )
        }

        // Determine the instance type to use. If provided use that one. Otherwise, for `x86_64` use `m5.large`
        // and for `aarch64` use `m6g.large`
        let instance_type = if let Some(instance_type) = request.configuration.instance_type {
            instance_type
        } else {
            instance_type(&ec2_client, &request.configuration.node_ami, &memo).await?
        };

        // Determine the instance profile name to use.
        let instance_profile_name =
            if let Some(instance_profile_name) = request.configuration.instance_profile_name {
                instance_profile_name
            } else {
                instance_profile_name(&iam_client, &memo).await?
            };

        // Run the ec2 instances
        let instance_count = request
            .configuration
            .instance_count
            .unwrap_or(DEFAULT_INSTANCE_COUNT);
        let run_instances = ec2_client
            .run_instances()
            .min_count(instance_count)
            .max_count(instance_count)
            .subnet_id(first_subnet_id(&private_subnet_ids, &memo)?)
            .set_security_group_ids(Some(security_groups))
            .image_id(request.configuration.node_ami)
            .instance_type(InstanceType::from(instance_type.as_str()))
            .tag_specifications(tag_specifications(cluster_name))
            .user_data(userdata(endpoint, cluster_name, certificate))
            .iam_instance_profile(
                IamInstanceProfileSpecification::builder()
                    .name(instance_profile_name)
                    .build(),
            );

        let instances = run_instances
            .send()
            .await
            .context(resources_situation(&memo), "Failed to create instances")?
            .instances
            .context(Resources::Orphaned, "Results missing instances field")?;
        let mut instance_ids = HashSet::default();
        for instance in instances {
            instance_ids.insert(instance.instance_id.clone().ok_or_else(|| {
                ProviderError::new_with_context(
                    Resources::Orphaned,
                    "Instance missing instance_id field",
                )
            })?);
        }

        // We are done, set our custom status to say so.
        memo.current_status = "Instance(s) Created".into();
        memo.region = region;
        client
            .send_info(memo.clone())
            .await
            .context(memo, "Error sending final creation message")?;

        // Return a description of the batch of robots that we created.
        Ok(CreatedEc2Instances { ids: instance_ids })
    }
}

async fn setup_env<I>(
    client: &I,
    aws_secret_name: &SecretName,
    memo: &ProductionMemo,
) -> ProviderResult<()>
where
    I: InfoClient,
{
    let aws_secret = client
        .get_secret(aws_secret_name)
        .await
        .context(memo, format!("Error getting secret '{}'", aws_secret_name))?;

    let access_key_id = String::from_utf8(
        aws_secret
            .get("access-key-id")
            .context(
                memo,
                format!("access-key-id missing from secret '{}'", aws_secret_name),
            )?
            .to_owned(),
    )
    .context(memo, "Could not convert access-key-id to String")?;
    let secret_access_key = String::from_utf8(
        aws_secret
            .get("secret-access-key")
            .context(
                memo,
                format!(
                    "secret-access-key missing from secret '{}'",
                    aws_secret_name
                ),
            )?
            .to_owned(),
    )
    .context(memo, "Could not convert secret-access-key to String")?;

    env::set_var("AWS_ACCESS_KEY_ID", access_key_id);
    env::set_var("AWS_SECRET_ACCESS_KEY", secret_access_key);

    Ok(())
}

async fn eks_subnet_ids(
    eks_client: &aws_sdk_eks::Client,
    cluster_name: &str,
    memo: &ProductionMemo,
) -> ProviderResult<Vec<String>> {
    let describe_results = eks_client
        .describe_cluster()
        .name(cluster_name)
        .send()
        .await
        .context(memo, "Unable to get eks describe cluster")?;

    // Extract the subnet ids from the cluster.
    describe_results
        .cluster
        .as_ref()
        .context(memo, "Response missing cluster field")?
        .resources_vpc_config
        .as_ref()
        .context(memo, "Cluster missing resources_vpc_config field")?
        .subnet_ids
        .as_ref()
        .context(memo, "resources_vpc_config missing subnet ids")
        .map(|ids| ids.clone())
}

async fn endpoint(
    eks_client: &aws_sdk_eks::Client,
    cluster_name: &str,
    memo: &ProductionMemo,
) -> ProviderResult<String> {
    let describe_results = eks_client
        .describe_cluster()
        .name(cluster_name)
        .send()
        .await
        .context(memo, "Unable to get eks describe cluster")?;
    // Extract the apiserver endpoint from the cluster.
    describe_results
        .cluster
        .as_ref()
        .context(memo, "Results missing cluster field")?
        .endpoint
        .as_ref()
        .context(memo, "Cluster missing endpoint field")
        .map(|ids| ids.clone())
}

async fn certificate(
    eks_client: &aws_sdk_eks::Client,
    cluster_name: &str,
    memo: &ProductionMemo,
) -> ProviderResult<String> {
    let describe_results = eks_client
        .describe_cluster()
        .name(cluster_name)
        .send()
        .await
        .context(memo, "Unable to get eks describe cluster")?;

    // Extract the certificate authority from the cluster.
    describe_results
        .cluster
        .as_ref()
        .context(memo, "Results missing cluster field")?
        .certificate_authority
        .as_ref()
        .context(memo, "Cluster missing certificate_authority field")?
        .data
        .as_ref()
        .context(memo, "Certificate authority missing data")
        .map(|ids| ids.clone())
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
    memo: &ProductionMemo,
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
        .context(memo, "Unable to get private subnet ids")?;
    describe_results
        .subnets
        .context(memo, "Results missing subnets field")
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
            SecurityGroupType::ClusterShared => "clustershared",
            SecurityGroupType::ControlPlane => "controlplane",
        };
        format!("*{}-{}*", cluster_name, sg)
    }
}

async fn security_group(
    ec2_client: &aws_sdk_ec2::Client,
    cluster_name: &str,
    security_group_type: SecurityGroupType,
    memo: &ProductionMemo,
) -> ProviderResult<Vec<SecurityGroup>> {
    // Extract the security groups.
    let describe_results = ec2_client
        .describe_security_groups()
        .filters(
            Filter::builder()
                .name("tag:Name")
                .values(security_group_type.tag(cluster_name))
                .build(),
        )
        .send()
        .await
        .context(
            memo,
            format!("Unable to get {:?} security group", security_group_type),
        )?;

    describe_results
        .security_groups
        .context(memo, "Results missing security_groups field")
}

async fn instance_type(
    ec2_client: &aws_sdk_ec2::Client,
    node_ami: &str,
    memo: &ProductionMemo,
) -> ProviderResult<String> {
    let arch = ec2_client
        .describe_images()
        .image_ids(node_ami)
        .send()
        .await
        .context(memo, "Unable to get ami architecture")?
        .images
        .context(memo, "Unable to get ami architecture")?
        .get(0)
        .context(memo, "Unable to get ami architecture")?
        .architecture
        .clone()
        .context(memo, "Ami has no architecture")?;

    Ok(match arch {
        ArchitectureValues::X8664 => "m5.large",
        ArchitectureValues::Arm64 => "m6g.large",
        _ => "m6g.large",
    }
    .to_string())
}

async fn instance_profile_name(
    iam_client: &aws_sdk_iam::Client,
    memo: &ProductionMemo,
) -> ProviderResult<String> {
    let list_result = iam_client
        .list_instance_profiles()
        .send()
        .await
        .context(memo, "Unable to list instance profiles")?;
    list_result
        .instance_profiles
        .as_ref()
        .context(memo, "No instance profiles found")?
        .iter()
        .find(|x| {
            x.instance_profile_name
                .as_ref()
                .unwrap_or(&"".to_string())
                .contains("NodeInstanceProfile")
        })
        .as_ref()
        .context(memo, "Node instance profile not found")?
        .instance_profile_name
        .as_ref()
        .context(
            memo,
            "Node instance profile missing instance_profile_name field",
        )
        .map(|profile| profile.clone())
}

fn tag_specifications(cluster_name: &str) -> TagSpecification {
    TagSpecification::builder()
        .resource_type(ResourceType::Instance)
        .tags(
            Tag::builder()
                .key("Name")
                .value(format!("{}_node", cluster_name))
                .build(),
        )
        .tags(
            Tag::builder()
                .key(format!("kubernetes.io/cluster/{}", cluster_name))
                .value("owned")
                .build(),
        )
        .build()
}

fn first_subnet_id(
    subnet_ids: &[aws_sdk_ec2::model::Subnet],
    memo: &ProductionMemo,
) -> ProviderResult<String> {
    subnet_ids
        .get(0)
        .as_ref()
        .context(memo, "There are no private subnet ids")?
        .subnet_id
        .as_ref()
        .cloned()
        .context(memo, "Subnet missing subnet_id field")
}

fn userdata(endpoint: &str, cluster_name: &str, certificate: &str) -> String {
    base64::encode(format!(
        r#"[settings.updates]
ignore-waves = true
    
[settings.kubernetes]
api-server = "{}"
cluster-name = "{}"
cluster-certificate = "{}""#,
        endpoint, cluster_name, certificate
    ))
}

/// This is the object that will destroy ec2 instances.
pub struct Ec2Destroyer {}

#[async_trait::async_trait]
impl Destroy for Ec2Destroyer {
    type Request = Ec2ProductionRequest;
    type Info = ProductionMemo;
    type Resource = CreatedEc2Instances;

    async fn destroy<I>(
        &self,
        _request: Option<Spec<Self::Request>>,
        resource: Option<Self::Resource>,
        client: &I,
    ) -> ProviderResult<()>
    where
        I: InfoClient,
    {
        let mut memo: ProductionMemo = client.get_info().await.map_err(|e| {
            ProviderError::new_with_source_and_context(
                Resources::Unknown,
                "Unable to get info from client",
                e,
            )
        })?;

        // Create a set of IDs to iterate over and destroy. Also ensure that the memo's IDs match.
        let ids = if let Some(resource) = resource {
            resource.ids
        } else {
            memo.clone().instance_ids
        };

        // Write aws credentials if we need them so we can run eksctl
        if let Some(aws_secret_name) = &memo.aws_secret_name {
            setup_env(client, aws_secret_name, &memo).await?;
        }

        let region_provider =
            RegionProviderChain::first_try(Some(Region::new(memo.region.clone())));
        let shared_config = aws_config::from_env().region(region_provider).load().await;
        let ec2_client = aws_sdk_ec2::Client::new(&shared_config);
        let _terminate_results = ec2_client
            .terminate_instances()
            .set_instance_ids(Some(Vec::from_iter(ids.clone())))
            .send()
            .await
            .map_err(|e| {
                ProviderError::new_with_source_and_context(
                    resources_situation(&memo),
                    "Failed to terminate instances",
                    e,
                )
            })?;

        for id in ids {
            memo.instance_ids.remove(&id);
        }

        memo.current_status = "Instances deleted".into();
        client.send_info(memo.clone()).await.map_err(|e| {
            ProviderError::new_with_source_and_context(
                resources_situation(&memo),
                "Error sending final destruction message",
                e,
            )
        })?;

        Ok(())
    }
}

/// When something goes wrong, we need to let the controller know whether or not we have existing
/// instances out there that need to be destroyed. We can do this by checking our `ProductionMemo`.
fn resources_situation(memo: &ProductionMemo) -> Resources {
    if memo.instance_ids.is_empty() {
        Resources::Clear
    } else {
        Resources::Remaining
    }
}

impl AsResources for ProductionMemo {
    fn as_resources(&self) -> Resources {
        resources_situation(self)
    }
}

impl AsResources for &ProductionMemo {
    fn as_resources(&self) -> Resources {
        resources_situation(self)
    }
}
