use aws_config::meta::region::RegionProviderChain;
use aws_sdk_ec2::model::Filter;
use aws_sdk_ec2::SdkError;
use aws_sdk_ecs::Region;
use aws_sdk_iam::error::{GetInstanceProfileError, GetInstanceProfileErrorKind};
use aws_sdk_iam::output::GetInstanceProfileOutput;
use bottlerocket_agents::{setup_resource_env, CreationPolicy, AWS_CREDENTIALS_SECRET_NAME};
use model::{Configuration, SecretName};
use resource_agent::clients::InfoClient;
use resource_agent::provider::{
    Create, Destroy, IntoProviderError, ProviderError, ProviderResult, Resources, Spec,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// The default region for the cluster.
const DEFAULT_REGION: &str = "us-west-2";
/// The ecs instance profile name.
const IAM_INSTANCE_PROFILE_NAME: &str = "testsys-bottlerocket-aws-ecsInstanceRole";

/// The configuration information for a Ecs instance provider.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClusterConfig {
    /// The name of the ecs cluster to create.
    cluster_name: String,

    /// The AWS region to create the cluster. If no value is provided `us-west-2` will be used.
    region: Option<String>,

    /// The vpc to use for this clusters subnet ids. If no value is provided the default vpc will be used.
    vpc: Option<String>,

    /// Determines if an ecs instance role should be created. If no creation policy is present, no instance role
    /// will be created.
    iam_instance_creation_policy: Option<CreationPolicy>,
}

impl Configuration for ClusterConfig {}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Memo {
    pub current_status: String,

    /// The name of the secret containing aws credentials.
    pub aws_secret_name: Option<SecretName>,

    /// The name of the cluster we created.
    pub cluster_name: Option<String>,

    /// The region the cluster is in.
    pub region: Option<String>,

    /// The `CreationPolicy` for the iam instance profile.
    pub iam_instance_creation_policy: Option<CreationPolicy>,
}

impl Configuration for Memo {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CreatedCluster {
    /// The name of the cluster we created.
    pub cluster_name: String,

    /// The region of the cluster.
    pub region: String,

    /// A public subnet id for this cluster.
    pub public_subnet_id: Option<String>,

    /// A private subnet id for this cluster.
    pub private_subnet_id: Option<String>,

    /// The iam instance role that was created for ecs
    pub iam_instance_profile_arn: Option<String>,
}

impl Configuration for CreatedCluster {}

pub struct EcsCreator {}

#[async_trait::async_trait]
impl Create for EcsCreator {
    type Config = ClusterConfig;
    type Info = Memo;
    type Resource = CreatedCluster;

    async fn create<I>(
        &self,
        spec: Spec<Self::Config>,
        client: &I,
    ) -> ProviderResult<Self::Resource>
    where
        I: InfoClient,
    {
        let mut memo: Memo = client
            .get_info()
            .await
            .context(Resources::Clear, "Unable to get info from client")?;

        let region = spec
            .configuration
            .region
            .as_ref()
            .unwrap_or(&DEFAULT_REGION.to_string())
            .to_string();

        // Write aws credentials if we have them so we can use AWS APIs.
        if let Some(aws_secret_name) = spec.secrets.get(AWS_CREDENTIALS_SECRET_NAME) {
            setup_resource_env(client, aws_secret_name, Resources::Clear).await?;
            memo.aws_secret_name = Some(aws_secret_name.clone());
        }

        let region_provider = RegionProviderChain::first_try(Some(Region::new(region.to_string())));
        let config = aws_config::from_env().region(region_provider).load().await;
        let ecs_client = aws_sdk_ecs::Client::new(&config);
        let iam_client = aws_sdk_iam::Client::new(&config);

        ecs_client
            .create_cluster()
            .cluster_name(&spec.configuration.cluster_name)
            .send()
            .await
            .context(Resources::Clear, "The cluster could not be created.")?;

        let iam_arn = match spec.configuration.iam_instance_creation_policy {
            Some(creation_policy) => {
                Some(create_iam_instance_profile(&iam_client, creation_policy).await?)
            }
            None => None,
        };

        let created_cluster = created_cluster(
            &spec.configuration.cluster_name,
            region.clone(),
            spec.configuration.vpc,
            iam_arn,
        )
        .await?;

        memo.current_status = "Cluster Created".into();
        memo.cluster_name = Some(spec.configuration.cluster_name);
        memo.region = Some(region);
        memo.iam_instance_creation_policy = spec.configuration.iam_instance_creation_policy;
        client.send_info(memo.clone()).await.context(
            Resources::Remaining,
            "Error sending cluster created message",
        )?;

        Ok(created_cluster)
    }
}

async fn create_iam_instance_profile(
    iam_client: &aws_sdk_iam::Client,
    creation_policy: CreationPolicy,
) -> ProviderResult<String> {
    let get_instance_profile_result = iam_client
        .get_instance_profile()
        .instance_profile_name(IAM_INSTANCE_PROFILE_NAME)
        .send()
        .await;
    match (creation_policy, exists(get_instance_profile_result)) {
        (CreationPolicy::Never, false) => Err(ProviderError::new_with_context(
            Resources::Remaining,
            "Instance profile creation policy is `Never`, but profile doesn't exist.",
        )),
        (CreationPolicy::Create, true) => Err(ProviderError::new_with_context(
            Resources::Remaining,
            "Instance profile creation policy is `Create`, but profile already exists.",
        )),
        (CreationPolicy::Create, false) | (CreationPolicy::IfNotExists, false) => {
            iam_client
                .create_role()
                .role_name(IAM_INSTANCE_PROFILE_NAME)
                .assume_role_policy_document(ecs_role_policy_document())
                .send()
                .await
                .context(Resources::Remaining, "Unable to create new role.")?;
            iam_client
                .attach_role_policy()
                .role_name(IAM_INSTANCE_PROFILE_NAME)
                .policy_arn("arn:aws:iam::aws:policy/AmazonSSMManagedInstanceCore")
                .send()
                .await
                .context(Resources::Remaining, "Unable to attach AmazonSSM policy")?;
            iam_client
                .attach_role_policy()
                .role_name(IAM_INSTANCE_PROFILE_NAME)
                .policy_arn(
                    "arn:aws:iam::aws:policy/service-role/AmazonEC2ContainerServiceforEC2Role",
                )
                .send()
                .await
                .context(
                    Resources::Remaining,
                    "Unable to attach AmazonEC2ContainerServiceforEC2Role policy",
                )?;
            iam_client
                .create_instance_profile()
                .instance_profile_name(IAM_INSTANCE_PROFILE_NAME)
                .send()
                .await
                .context(Resources::Remaining, "Unable to create instance profile")?;
            iam_client
                .add_role_to_instance_profile()
                .instance_profile_name(IAM_INSTANCE_PROFILE_NAME)
                .role_name(IAM_INSTANCE_PROFILE_NAME)
                .send()
                .await
                .context(
                    Resources::Remaining,
                    "Unable to add role to instance profile",
                )?;
            // TODO: find a better way to allow propagation than a sleep.
            tokio::time::sleep(Duration::from_secs(60)).await;
            instance_profile_arn(iam_client).await
        }
        (CreationPolicy::Never, true) | (CreationPolicy::IfNotExists, true) => {
            instance_profile_arn(iam_client).await
        }
    }
}

fn exists(result: Result<GetInstanceProfileOutput, SdkError<GetInstanceProfileError>>) -> bool {
    if let Err(SdkError::ServiceError { err, raw: _ }) = result {
        if matches!(
            &err.kind,
            GetInstanceProfileErrorKind::NoSuchEntityException(_)
        ) {
            return false;
        }
    }
    true
}

async fn instance_profile_arn(iam_client: &aws_sdk_iam::Client) -> ProviderResult<String> {
    iam_client
        .get_instance_profile()
        .instance_profile_name(IAM_INSTANCE_PROFILE_NAME)
        .send()
        .await
        .context(Resources::Remaining, "Unable to get instance profile.")?
        .instance_profile()
        .and_then(|instance_profile| instance_profile.arn())
        .context(
            Resources::Remaining,
            "Instance profile does not contain an arn.",
        )
        .map(|arn| arn.to_string())
}

async fn created_cluster(
    cluster_name: &str,
    region: String,
    vpc: Option<String>,
    iam_instance_profile_arn: Option<String>,
) -> ProviderResult<CreatedCluster> {
    let region_provider = RegionProviderChain::first_try(Some(Region::new(region.clone())));
    let shared_config = aws_config::from_env().region(region_provider).load().await;
    let ec2_client = aws_sdk_ec2::Client::new(&shared_config);

    let vpc = match vpc {
        Some(vpc) => vpc,
        None => default_vpc(&ec2_client).await?,
    };

    let public_subnet_ids = subnet_ids(&ec2_client, SubnetType::Public, &vpc).await?;
    let private_subnet_ids = subnet_ids(&ec2_client, SubnetType::Private, &vpc).await?;

    Ok(CreatedCluster {
        cluster_name: cluster_name.to_string(),
        region,
        public_subnet_id: first_subnet_id(&public_subnet_ids),
        private_subnet_id: first_subnet_id(&private_subnet_ids),
        iam_instance_profile_arn,
    })
}

fn first_subnet_id(subnet_ids: &[String]) -> Option<String> {
    subnet_ids.get(0).map(|id| id.to_string())
}

async fn default_vpc(ec2_client: &aws_sdk_ec2::Client) -> ProviderResult<String> {
    Ok(ec2_client
        .describe_vpcs()
        .filters(Filter::builder().name("isDefault").values("true").build())
        .send()
        .await
        .context(Resources::Remaining, "VPC list is missing.")?
        .vpcs()
        .and_then(|vpcs| vpcs.first().and_then(|vpc| vpc.vpc_id()))
        .context(Resources::Remaining, "The default vpc has no vpc id.")?
        .to_string())
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum SubnetType {
    Public,
    Private,
}

async fn subnet_ids(
    ec2_client: &aws_sdk_ec2::Client,
    subnet_type: SubnetType,
    vpc_id: &str,
) -> ProviderResult<Vec<String>> {
    Ok(ec2_client
        .describe_subnets()
        .filters(Filter::builder().name("vpc-id").values(vpc_id).build())
        .send()
        .await
        .context(Resources::Remaining, "Unable to get subnet information.")?
        .subnets()
        .context(Resources::Remaining, "Unable to get subnets.")?
        .iter()
        .filter_map(
            |subnet| match (subnet.map_public_ip_on_launch(), &subnet_type) {
                (Some(true), &SubnetType::Public) => subnet.subnet_id().map(|id| id.to_owned()),
                (Some(false), &SubnetType::Private) => subnet.subnet_id().map(|id| id.to_owned()),
                _ => None,
            },
        )
        .collect())
}

pub struct EcsDestroyer {}

#[async_trait::async_trait]
impl Destroy for EcsDestroyer {
    type Config = ClusterConfig;
    type Info = Memo;
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
        let mut memo: Memo = client
            .get_info()
            .await
            .context(Resources::Remaining, "Unable to get info from client")?;

        // Write aws credentials if we need them
        if let Some(aws_secret_name) = &memo.aws_secret_name {
            setup_resource_env(client, aws_secret_name, Resources::Remaining).await?;
        }
        let region = memo
            .clone()
            .region
            .unwrap_or_else(|| DEFAULT_REGION.to_string());

        let region_provider = RegionProviderChain::first_try(Some(Region::new(region.to_string())));
        let config = aws_config::from_env().region(region_provider).load().await;
        let ecs_client = aws_sdk_ecs::Client::new(&config);
        let iam_client = aws_sdk_iam::Client::new(&config);

        if memo.iam_instance_creation_policy == Some(CreationPolicy::Create) {
            iam_client
                .remove_role_from_instance_profile()
                .instance_profile_name(IAM_INSTANCE_PROFILE_NAME)
                .role_name(IAM_INSTANCE_PROFILE_NAME)
                .send()
                .await
                .context(
                    Resources::Remaining,
                    "Unable to remove role from instance profile.",
                )?;
            iam_client
                .delete_instance_profile()
                .instance_profile_name(IAM_INSTANCE_PROFILE_NAME)
                .send()
                .await
                .context(Resources::Remaining, "Unable to delete instance profile.")?;
            iam_client
                .detach_role_policy()
                .role_name(IAM_INSTANCE_PROFILE_NAME)
                .policy_arn(
                    "arn:aws:iam::aws:policy/service-role/AmazonEC2ContainerServiceforEC2Role",
                )
                .send()
                .await
                .context(
                    Resources::Remaining,
                    "Unable to detach AmazonEC2ContainerServiceforEC2Role",
                )?;
            iam_client
                .detach_role_policy()
                .role_name(IAM_INSTANCE_PROFILE_NAME)
                .policy_arn("arn:aws:iam::aws:policy/AmazonSSMManagedInstanceCore")
                .send()
                .await
                .context(
                    Resources::Remaining,
                    "Unable to detach AmazonSSMManagedInstanceCore",
                )?;
            iam_client
                .delete_role()
                .role_name(IAM_INSTANCE_PROFILE_NAME)
                .send()
                .await
                .context(Resources::Remaining, "Unable to delete iam role.")?;
        }

        if let Some(cluster_name) = &memo.cluster_name {
            ecs_client
                .delete_cluster()
                .cluster(cluster_name)
                .send()
                .await
                .context(Resources::Unknown, "The cluster could not be deleted.")?;

            memo.current_status = "Cluster deleted".into();
            if let Err(e) = client.send_info(memo.clone()).await {
                eprintln!(
                    "Cluster deleted but failed to send info message to k8s: {}",
                    e
                )
            }
        }

        Ok(())
    }
}

fn ecs_role_policy_document() -> String {
    r#"{
    "Version": "2008-10-17",
    "Statement": [
        {
        "Sid": "",
        "Effect": "Allow",
        "Principal": {
            "Service": "ec2.amazonaws.com"
        },
        "Action": "sts:AssumeRole"
        }
    ]
}"#
    .to_string()
}
