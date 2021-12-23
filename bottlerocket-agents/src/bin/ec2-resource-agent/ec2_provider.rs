use aws_config::meta::region::RegionProviderChain;
use aws_sdk_ec2::model::{
    ArchitectureValues, Filter, IamInstanceProfileSpecification, InstanceType, ResourceType, Tag,
    TagSpecification,
};
use aws_sdk_ec2::Region;
use bottlerocket_agents::{ClusterInfo, UserData, AWS_CREDENTIALS_SECRET_NAME};
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
use std::time::Duration;
use uuid::Uuid;

/// The default number of instances to spin up.
const DEFAULT_INSTANCE_COUNT: i32 = 2;
/// The tag name for the uuid used to create instances.
const INSTANCE_UUID_TAG_NAME: &str = "testsys-ec2-uuid";

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

    /// A UUID that was used to tag the instances in case we lose our created instance IDs.
    pub uuid_tag: Option<String>,
}

impl Configuration for ProductionMemo {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Ec2Config {
    /// The AMI ID of the AMI to use for the worker nodes.
    node_ami: String,

    /// The number of instances to create. If no value is provided 2 instances will be created.
    instance_count: Option<i32>,

    /// The type of instance to spin up. m5.large is recommended for x86_64 and m6g.large is
    /// recommended for arm64. If no value is provided the recommended type will be used.
    instance_type: Option<String>,

    /// The type of subnet that will be used for the ec2 instances. If no type is provided the first
    /// private subnet will be used.
    #[serde(default)]
    subnet_type: SubnetType,

    /// All of the cluster based information needed to run instances.
    cluster: ClusterInfo,

    /// Uses the `UserData` enum to determine what user data should be applied to the instances.
    user_data: UserData,
}

impl Configuration for Ec2Config {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum SubnetType {
    Public,
    Private,
}

impl Default for SubnetType {
    fn default() -> Self {
        Self::Private
    }
}

/// Once we have fulfilled the `Create` request, we return information about the batch of ec2 instances we
/// created.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CreatedEc2Instances {
    /// The ids of all created instances
    pub ids: HashSet<String>,
}

impl Configuration for CreatedEc2Instances {}

pub struct Ec2Creator {}

#[async_trait::async_trait]
impl Create for Ec2Creator {
    type Config = Ec2Config;
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
        let mut memo: ProductionMemo = client
            .get_info()
            .await
            .context(Resources::Unknown, "Unable to get info from info client")?;

        // Set the uuid before we do anything so we know it is stored.
        let instance_uuid = Uuid::new_v4().to_string();
        memo.uuid_tag = Some(instance_uuid.to_string());
        client
            .send_info(memo.clone())
            .await
            .context(&memo, "Error storing uuid in info client")?;

        // Write aws credentials if we need them so we can run eksctl
        if let Some(aws_secret_name) = spec.secrets.get(AWS_CREDENTIALS_SECRET_NAME) {
            setup_env(client, aws_secret_name, &memo).await?;
            memo.aws_secret_name = Some(aws_secret_name.clone());
        }

        let cluster = &spec.configuration.cluster;

        // Setup aws_sdk_config and clients.
        let region_provider =
            RegionProviderChain::first_try(Some(Region::new(cluster.region.clone())));
        let shared_config = aws_config::from_env().region(region_provider).load().await;
        let ec2_client = aws_sdk_ec2::Client::new(&shared_config);

        // Determine the instance type to use. If provided use that one. Otherwise, for `x86_64` use `m5.large`
        // and for `aarch64` use `m6g.large`
        let instance_type = if let Some(instance_type) = spec.configuration.instance_type {
            instance_type
        } else {
            instance_type(&ec2_client, &spec.configuration.node_ami, &memo).await?
        };

        let subnet_id = first_subnet_id(
            match spec.configuration.subnet_type {
                SubnetType::Public => &spec.configuration.cluster.public_subnet_ids,
                SubnetType::Private => &spec.configuration.cluster.private_subnet_ids,
            },
            &memo,
        )?;

        // Run the ec2 instances
        let instance_count = spec
            .configuration
            .instance_count
            .unwrap_or(DEFAULT_INSTANCE_COUNT);
        let run_instances = ec2_client
            .run_instances()
            .min_count(instance_count)
            .max_count(instance_count)
            .subnet_id(subnet_id)
            .set_security_group_ids(Some(cluster.security_groups.clone()))
            .image_id(spec.configuration.node_ami)
            .instance_type(InstanceType::from(instance_type.as_str()))
            .tag_specifications(tag_specifications(&cluster.name, &instance_uuid))
            .user_data(&spec.configuration.user_data.user_data(&cluster.name))
            .iam_instance_profile(
                IamInstanceProfileSpecification::builder()
                    .arn(&cluster.iam_instance_profile_arn)
                    .build(),
            );

        let instances = run_instances
            .send()
            .await
            .context(resources_situation(&memo), "Failed to create instances")?
            .instances
            .context(Resources::Remaining, "Results missing instances field")?;
        let mut instance_ids = HashSet::default();
        for instance in instances {
            instance_ids.insert(instance.instance_id.clone().ok_or_else(|| {
                ProviderError::new_with_context(
                    Resources::Remaining,
                    "Instance missing instance_id field",
                )
            })?);
        }

        // Ensure the instances reach a running state.
        tokio::time::timeout(
            Duration::from_secs(60),
            wait_for_conforming_instances(
                &ec2_client,
                &instance_ids,
                DesiredInstanceState::Running,
                &memo,
            ),
        )
        .await
        .context(
            &memo,
            "Timed-out waiting for instances to reach the `running` state.",
        )??;

        // We are done, set our custom status to say so.
        memo.current_status = "Instance(s) Created".into();
        memo.region = cluster.region.clone();
        memo.instance_ids = instance_ids.clone();
        client
            .send_info(memo.clone())
            .await
            .context(memo, "Error sending final creation message")?;

        // Return the ids for the created instances.
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

fn tag_specifications(cluster_name: &str, instance_uuid: &str) -> TagSpecification {
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
        .tags(
            Tag::builder()
                .key(INSTANCE_UUID_TAG_NAME)
                .value(instance_uuid)
                .build(),
        )
        .build()
}

fn first_subnet_id(subnet_ids: &[String], memo: &ProductionMemo) -> ProviderResult<String> {
    subnet_ids
        .get(0)
        .map(|id| id.to_string())
        .context(memo, "There are no private subnet ids")
}

#[derive(Debug)]
enum DesiredInstanceState {
    Running,
    Terminated,
}

impl DesiredInstanceState {
    fn filter(&self) -> Filter {
        let filter = Filter::builder()
            .name("instance-state-name")
            .values("pending")
            .values("shutting-down")
            .values("stopping")
            .values("stopped")
            .values(match self {
                DesiredInstanceState::Running => "terminated",
                DesiredInstanceState::Terminated => "running",
            });

        filter.build()
    }
}

async fn non_conforming_instances(
    ec2_client: &aws_sdk_ec2::Client,
    instance_ids: &HashSet<String>,
    desired_instance_state: &DesiredInstanceState,
    memo: &ProductionMemo,
) -> ProviderResult<Vec<String>> {
    let mut describe_result = ec2_client
        .describe_instance_status()
        .filters(desired_instance_state.filter())
        .set_instance_ids(Some(Vec::from_iter(instance_ids.clone())))
        .include_all_instances(true)
        .send()
        .await
        .context(
            memo,
            format!(
                "Unable to list instances in the '{:?}' state.",
                desired_instance_state
            ),
        )?;
    let non_conforming_instances = describe_result
        .instance_statuses
        .as_mut()
        .context(memo, "No instance statuses were provided.")?;

    Ok(non_conforming_instances
        .iter_mut()
        .filter_map(|instance_status| instance_status.instance_id.clone())
        .collect())
}

async fn wait_for_conforming_instances(
    ec2_client: &aws_sdk_ec2::Client,
    instance_ids: &HashSet<String>,
    desired_instance_state: DesiredInstanceState,
    memo: &ProductionMemo,
) -> ProviderResult<()> {
    loop {
        if !non_conforming_instances(ec2_client, instance_ids, &desired_instance_state, memo)
            .await?
            .is_empty()
        {
            tokio::time::sleep(Duration::from_millis(1000)).await;
            continue;
        }
        return Ok(());
    }
}

// Find all running instances with the uuid for this resource.
async fn get_instances_by_uuid(
    ec2_client: &aws_sdk_ec2::Client,
    uuid: &str,
    memo: &ProductionMemo,
) -> ProviderResult<HashSet<String>> {
    let mut describe_result = ec2_client
        .describe_instances()
        .filters(
            Filter::builder()
                .name("tag")
                .values(format!("{}={}", INSTANCE_UUID_TAG_NAME, uuid))
                .build(),
        )
        .send()
        .await
        .context(memo, "Unable to get instances.")?;
    let instances = describe_result
        .reservations
        .as_mut()
        .context(memo, "No instances were provided.")?;

    Ok(instances
        .iter_mut()
        // Extract the vec of `Instance`s from each `Reservation`
        .filter_map(|reservation| reservation.instances.as_ref())
        // Combine all `Instance`s into one iterator no matter which `Reservation` they
        // came from.
        .flatten()
        // Extract the instance id from each `Instance`.
        .filter_map(|instance| instance.instance_id.clone())
        .collect())
}

/// This is the object that will destroy ec2 instances.
pub struct Ec2Destroyer {}

#[async_trait::async_trait]
impl Destroy for Ec2Destroyer {
    type Config = Ec2Config;
    type Info = ProductionMemo;
    type Resource = CreatedEc2Instances;

    async fn destroy<I>(
        &self,
        _: Option<Spec<Self::Config>>,
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
        let mut ids = if let Some(resource) = resource {
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

        // If we don't have any instances to delete make sure there weren't any instances
        // with the uuid.
        if ids.is_empty() {
            if let Some(uuid) = &memo.uuid_tag {
                ids = get_instances_by_uuid(&ec2_client, uuid, &memo).await?;
            }
        }

        if ids.is_empty() {
            return Ok(());
        }

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

        for id in &ids {
            memo.instance_ids.remove(id);
        }

        memo.current_status = "Instances deleted".into();
        client.send_info(memo.clone()).await.map_err(|e| {
            ProviderError::new_with_source_and_context(
                resources_situation(&memo),
                "Error sending final destruction message",
                e,
            )
        })?;

        // Ensure the instances reach a terminated state.
        tokio::time::timeout(
            Duration::from_secs(300),
            wait_for_conforming_instances(
                &ec2_client,
                &ids,
                DesiredInstanceState::Terminated,
                &memo,
            ),
        )
        .await
        .context(
            &memo,
            "Timed-out waiting for instances to reach the `terminated` state.",
        )??;
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
