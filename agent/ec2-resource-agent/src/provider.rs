use aws_config::meta::region::RegionProviderChain;
use aws_sdk_ec2::model::{
    ArchitectureValues, IamInstanceProfileSpecification, InstanceType, ResourceType, Tag,
    TagSpecification,
};
use aws_sdk_ec2::Region;
use ec2_resource_agent::ClusterInfo;
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

    /// The number of instances to create. If no value is provided 2 instances will be created.
    instance_count: Option<i32>,

    /// The type of instance to spin up. m5.large is recommended for x86_64 and m6g.large is
    /// recommended for arm64. If no value is provided the recommended type will be used.
    instance_type: Option<String>,

    /// All of the cluster based information needed to run instances.
    cluster: ClusterInfo,
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

        // Write aws credentials if we need them so we can run eksctl
        if let Some(aws_secret_name) = request.secrets.get("aws-credentials") {
            setup_env(client, aws_secret_name, &memo).await?;
            memo.aws_secret_name = Some(aws_secret_name.clone());
        }

        let cluster = &request.configuration.cluster;

        // Setup aws_sdk_config and clients.
        let region_provider =
            RegionProviderChain::first_try(Some(Region::new(cluster.region.clone())));
        let shared_config = aws_config::from_env().region(region_provider).load().await;
        let ec2_client = aws_sdk_ec2::Client::new(&shared_config);

        let mut security_groups = vec![];
        security_groups.append(&mut cluster.nodegroup_sg.clone());
        security_groups.append(&mut cluster.clustershared_sg.clone());

        // Determine the instance type to use. If provided use that one. Otherwise, for `x86_64` use `m5.large`
        // and for `aarch64` use `m6g.large`
        let instance_type = if let Some(instance_type) = request.configuration.instance_type {
            instance_type
        } else {
            instance_type(&ec2_client, &request.configuration.node_ami, &memo).await?
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
            .subnet_id(first_subnet_id(&cluster.private_subnet_ids, &memo)?)
            .set_security_group_ids(Some(security_groups))
            .image_id(request.configuration.node_ami)
            .instance_type(InstanceType::from(instance_type.as_str()))
            .tag_specifications(tag_specifications(&cluster.name))
            .user_data(userdata(
                &cluster.endpoint,
                &cluster.name,
                &cluster.certificate,
            ))
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
        memo.region = cluster.region.clone();
        client
            .send_info(memo.clone())
            .await
            .context(memo, "Error sending final creation message")?;

        // Return a description of the batch of instances that we created.
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

fn first_subnet_id(subnet_ids: &[String], memo: &ProductionMemo) -> ProviderResult<String> {
    subnet_ids
        .get(0)
        .map(|id| id.to_string())
        .context(memo, "There are no private subnet ids")
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
