use agent_utils::aws::{aws_resource_config, setup_resource_env};
use agent_utils::json_display;
use aws_sdk_ec2::client::fluent_builders::RunInstances;
use aws_sdk_ec2::error::RunInstancesError;
use aws_sdk_ec2::model::{
    ArchitectureValues, Filter, IamInstanceProfileSpecification, InstanceType, ResourceType, Tag,
    TagSpecification,
};
use aws_sdk_ec2::output::RunInstancesOutput;
use aws_sdk_ec2::types::SdkError;
use bottlerocket_types::agent_config::{ClusterType, Ec2Config, AWS_CREDENTIALS_SECRET_NAME};
use log::{debug, info, trace, warn};
use model::{Configuration, SecretName};
use resource_agent::clients::InfoClient;
use resource_agent::provider::{
    AsResources, Create, Destroy, IntoProviderError, ProviderError, ProviderResult, Resources, Spec,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
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

    /// The role that is assumed.
    pub assume_role: Option<String>,
}

impl Configuration for ProductionMemo {}

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
        debug!(
            "create is starting with the following spec:\n{}",
            json_display(&spec)
        );
        let mut memo: ProductionMemo = client
            .get_info()
            .await
            .context(Resources::Unknown, "Unable to get info from info client")?;

        // Set the uuid before we do anything so we know it is stored.
        let instance_uuid = Uuid::new_v4().to_string();
        info!(
            "Beginning instance creation with instance UUID: {}",
            instance_uuid
        );
        memo.uuid_tag = Some(instance_uuid.to_string());
        client
            .send_info(memo.clone())
            .await
            .context(&memo, "Error storing uuid in info client")?;

        // Write aws credentials if we need them so we can run eksctl
        if let Some(aws_secret_name) = spec.secrets.get(AWS_CREDENTIALS_SECRET_NAME) {
            setup_resource_env(client, aws_secret_name, memo.as_resources()).await?;
            memo.aws_secret_name = Some(aws_secret_name.clone());
        }

        memo.aws_secret_name = spec.secrets.get(AWS_CREDENTIALS_SECRET_NAME).cloned();
        memo.assume_role = spec.configuration.assume_role.clone();

        let shared_config = aws_resource_config(
            client,
            &spec.secrets.get(AWS_CREDENTIALS_SECRET_NAME),
            &spec.configuration.assume_role,
            &Some(spec.configuration.region.clone()),
            Resources::Clear,
        )
        .await?;
        let ec2_client = aws_sdk_ec2::Client::new(&shared_config);

        // Determine the instance type to use. If provided use that one. Otherwise, for `x86_64` use `m5.large`
        // and for `aarch64` use `m6g.large`
        let instance_type = if let Some(instance_type) = spec.configuration.instance_type {
            instance_type
        } else {
            instance_type(&ec2_client, &spec.configuration.node_ami, &memo).await?
        };
        info!("Using instance type '{}'", instance_type);

        // Run the ec2 instances
        let instance_count = spec
            .configuration
            .instance_count
            .unwrap_or(DEFAULT_INSTANCE_COUNT);
        info!("Creating {} instance(s)", instance_count);

        let run_instances = ec2_client
            .run_instances()
            .min_count(instance_count)
            .max_count(instance_count)
            .subnet_id(&spec.configuration.subnet_id)
            .set_security_group_ids(Some(spec.configuration.security_groups.clone()))
            .image_id(spec.configuration.node_ami)
            .instance_type(InstanceType::from(instance_type.as_str()))
            .tag_specifications(tag_specifications(
                &spec.configuration.cluster_type,
                &spec.configuration.cluster_name,
                &instance_uuid,
            ))
            .user_data(userdata(
                &spec.configuration.cluster_type,
                &spec.configuration.cluster_name,
                &spec.configuration.endpoint,
                &spec.configuration.certificate,
                &spec.configuration.cluster_dns_ip,
                &memo,
            )?)
            .iam_instance_profile(
                IamInstanceProfileSpecification::builder()
                    .arn(&spec.configuration.instance_profile_arn)
                    .build(),
            );

        info!("Starting instances");
        let run_instance_result = tokio::time::timeout(
            Duration::from_secs(360),
            wait_for_successful_run_instances(&run_instances),
        )
        .await
        .context(
            Resources::Clear,
            "Failed to run instances within the time limit",
        )?;

        let instances = run_instance_result
            .context(resources_situation(&memo), "Failed to create instances")?
            .instances
            .context(Resources::Remaining, "Results missing instances field")?;
        let mut instance_ids = HashSet::default();

        info!("Checking instance IDs");
        for instance in instances {
            instance_ids.insert(instance.instance_id.clone().ok_or_else(|| {
                ProviderError::new_with_context(
                    Resources::Remaining,
                    "Instance missing instance_id field",
                )
            })?);
        }

        // Ensure the instances reach a running state.
        info!("Waiting for instances to reach the running state");
        tokio::time::timeout(
            Duration::from_secs(300),
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
        memo.region = spec.configuration.region.clone();
        memo.instance_ids = instance_ids.clone();
        client
            .send_info(memo.clone())
            .await
            .context(&memo, "Error sending final creation message")?;

        info!("Done: instances {:?} are running", memo.instance_ids);
        // Return the ids for the created instances.
        Ok(CreatedEc2Instances { ids: instance_ids })
    }
}

async fn wait_for_successful_run_instances(
    run_instances: &RunInstances,
) -> Result<RunInstancesOutput, SdkError<RunInstancesError>> {
    loop {
        let run_instance_result = run_instances.clone().send().await;
        if let Err(SdkError::ServiceError { err, raw: _ }) = &run_instance_result {
            if matches!(&err.code(), Some("InvalidParameterValue")) {
                warn!(
                    "An error occured while trying to run instances '{}'. Retrying in 10s.",
                    err
                );
                tokio::time::sleep(Duration::from_secs(10)).await;
                info!("Rerunning run instances");
                continue;
            }
        };
        return run_instance_result;
    }
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

fn tag_specifications(
    cluster_type: &ClusterType,
    cluster_name: &str,
    instance_uuid: &str,
) -> TagSpecification {
    match cluster_type {
        ClusterType::Eks => TagSpecification::builder()
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
            .build(),
        ClusterType::Ecs => TagSpecification::builder()
            .resource_type(ResourceType::Instance)
            .tags(
                Tag::builder()
                    .key(INSTANCE_UUID_TAG_NAME)
                    .value(instance_uuid)
                    .build(),
            )
            .build(),
    }
}

fn userdata(
    cluster_type: &ClusterType,
    cluster_name: &str,
    endpoint: &Option<String>,
    certificate: &Option<String>,
    cluster_dns_ip: &Option<String>,
    memo: &ProductionMemo,
) -> ProviderResult<String> {
    Ok(match cluster_type {
        ClusterType::Eks => base64::encode(format!(
            r#"[settings.updates]
ignore-waves = true
    
[settings.kubernetes]
api-server = "{}"
cluster-name = "{}"
cluster-certificate = "{}"
cluster-dns-ip = "{}""#,
            endpoint
                .as_ref()
                .context(memo, "Server endpoint is required for eks clusters.")?,
            cluster_name,
            certificate
                .as_ref()
                .context(memo, "Cluster certificate is required for eks clusters.")?,
            cluster_dns_ip
                .as_ref()
                .context(memo, "Cluster DNS IP is required for eks clusters.")?,
        )),
        ClusterType::Ecs => base64::encode(format!(
            r#"[settings.ecs]
cluster = "{}""#,
            cluster_name,
        )),
    })
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

    let non_conforming_instances = non_conforming_instances
        .iter_mut()
        .filter_map(|instance_status| instance_status.instance_id.clone())
        .collect();

    trace!(
        "The following instances are not in the desired state '{:?}': {:?}",
        desired_instance_state,
        non_conforming_instances
    );
    Ok(non_conforming_instances)
}

async fn wait_for_conforming_instances(
    ec2_client: &aws_sdk_ec2::Client,
    instance_ids: &HashSet<String>,
    desired_instance_state: DesiredInstanceState,
    memo: &ProductionMemo,
) -> ProviderResult<()> {
    loop {
        if !non_conforming_instances(ec2_client, instance_ids, &desired_instance_state, memo)
            .await
            .map_err(|e| warn!("Error checking status of instances. Retrying: {}", e))
            .map_or(true, |ids| ids.is_empty())
        {
            trace!("Some instances are not ready, sleeping and trying again");
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

        let shared_config = aws_resource_config(
            client,
            &memo.aws_secret_name.as_ref(),
            &memo.assume_role,
            &Some(memo.region.clone()),
            Resources::Clear,
        )
        .await?;
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
            Duration::from_secs(600),
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
