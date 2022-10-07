use agent_utils::aws::aws_resource_config;
use aws_sdk_ec2::model::Filter;
use aws_sdk_ec2::types::SdkError;
use aws_sdk_iam::error::{GetInstanceProfileError, GetInstanceProfileErrorKind};
use aws_sdk_iam::output::GetInstanceProfileOutput;
use aws_types::sdk_config::SdkConfig;
use bottlerocket_types::agent_config::{EcsClusterConfig, AWS_CREDENTIALS_SECRET_NAME};
use log::{error, info};
use model::{Configuration, SecretName};
use resource_agent::clients::InfoClient;
use resource_agent::provider::{
    Create, Destroy, IntoProviderError, ProviderResult, Resources, Spec,
};
use serde::{Deserialize, Serialize};

/// The default region for the cluster.
const DEFAULT_REGION: &str = "us-west-2";
/// The ecs instance profile name.
const IAM_INSTANCE_PROFILE_NAME: &str = "testsys-bottlerocket-aws-ecsInstanceRole";

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Memo {
    pub current_status: String,

    /// The name of the secret containing aws credentials.
    pub aws_secret_name: Option<SecretName>,

    /// What role the agent is assuming.
    pub assume_role: Option<String>,

    /// The name of the cluster we created.
    pub cluster_name: Option<String>,

    /// The region the cluster is in.
    pub region: Option<String>,
}

impl Configuration for Memo {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CreatedCluster {
    /// The name of the cluster we created.
    pub cluster_name: String,

    /// The region of the cluster.
    pub region: String,

    /// A vector of public subnet ids for this cluster.
    pub public_subnet_ids: Vec<String>,

    /// A vector of private subnet ids for this cluster.
    pub private_subnet_ids: Vec<String>,

    /// The iam instance role that was created for ecs
    pub iam_instance_profile_arn: String,
}

impl Configuration for CreatedCluster {}

pub struct EcsCreator {}

#[async_trait::async_trait]
impl Create for EcsCreator {
    type Config = EcsClusterConfig;
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
        info!("Creating ECS cluster");
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

        memo.aws_secret_name = spec.secrets.get(AWS_CREDENTIALS_SECRET_NAME).cloned();
        memo.assume_role = spec.configuration.assume_role.clone();

        let config = aws_resource_config(
            client,
            &spec.secrets.get(AWS_CREDENTIALS_SECRET_NAME),
            &spec.configuration.assume_role,
            &spec.configuration.region,
            Resources::Clear,
        )
        .await?;
        let ecs_client = aws_sdk_ecs::Client::new(&config);
        let iam_client = aws_sdk_iam::Client::new(&config);

        info!("Creating cluster '{}'", spec.configuration.cluster_name);
        ecs_client
            .create_cluster()
            .cluster_name(&spec.configuration.cluster_name)
            .send()
            .await
            .context(Resources::Clear, "The cluster could not be created.")?;

        let iam_arn = match spec.configuration.iam_instance_profile_name {
            Some(iam_instance_profile_name) => {
                instance_profile_arn(&iam_client, &iam_instance_profile_name)
                    .await
                    .context(
                        Resources::Clear,
                        "The iam instance profile name was not found.",
                    )?
            }
            None => {
                info!("Creating instance profile");
                create_iam_instance_profile(&iam_client).await?
            }
        };

        info!("Getting cluster information");
        let created_cluster = created_cluster(
            &config,
            &spec.configuration.cluster_name,
            region.clone(),
            spec.configuration.vpc,
            iam_arn,
        )
        .await?;

        memo.current_status = "Cluster Created".into();
        memo.cluster_name = Some(spec.configuration.cluster_name);
        memo.region = Some(region);
        client.send_info(memo.clone()).await.context(
            Resources::Remaining,
            "Error sending cluster created message",
        )?;

        Ok(created_cluster)
    }
}

async fn create_iam_instance_profile(iam_client: &aws_sdk_iam::Client) -> ProviderResult<String> {
    let get_instance_profile_result = iam_client
        .get_instance_profile()
        .instance_profile_name(IAM_INSTANCE_PROFILE_NAME)
        .send()
        .await;
    if exists(get_instance_profile_result) {
        instance_profile_arn(iam_client, IAM_INSTANCE_PROFILE_NAME).await
    } else {
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
            .policy_arn("arn:aws:iam::aws:policy/service-role/AmazonEC2ContainerServiceforEC2Role")
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
        instance_profile_arn(iam_client, IAM_INSTANCE_PROFILE_NAME).await
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

async fn instance_profile_arn(
    iam_client: &aws_sdk_iam::Client,
    iam_instance_profile_name: &str,
) -> ProviderResult<String> {
    iam_client
        .get_instance_profile()
        .instance_profile_name(iam_instance_profile_name)
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
    shared_config: &SdkConfig,
    cluster_name: &str,
    region: String,
    vpc: Option<String>,
    iam_instance_profile_arn: String,
) -> ProviderResult<CreatedCluster> {
    let ec2_client = aws_sdk_ec2::Client::new(shared_config);

    let vpc = match vpc {
        Some(vpc) => vpc,
        None => default_vpc(&ec2_client).await?,
    };

    let public_subnet_ids = subnet_ids(&ec2_client, SubnetType::Public, &vpc).await?;
    let private_subnet_ids = subnet_ids(&ec2_client, SubnetType::Private, &vpc).await?;

    Ok(CreatedCluster {
        cluster_name: cluster_name.to_string(),
        region,
        public_subnet_ids,
        private_subnet_ids,
        iam_instance_profile_arn,
    })
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
    type Config = EcsClusterConfig;
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
        info!("Running destroy");
        let mut memo: Memo = client
            .get_info()
            .await
            .context(Resources::Remaining, "Unable to get info from client")?;

        let config = aws_resource_config(
            client,
            &memo.aws_secret_name.as_ref(),
            &memo.assume_role,
            &memo.region,
            Resources::Clear,
        )
        .await?;
        let ecs_client = aws_sdk_ecs::Client::new(&config);

        info!(
            "Deleting cluster '{}'",
            memo.cluster_name.as_deref().unwrap_or_default()
        );
        if let Some(cluster_name) = &memo.cluster_name {
            ecs_client
                .delete_cluster()
                .cluster(cluster_name)
                .send()
                .await
                .context(Resources::Unknown, "The cluster could not be deleted.")?;

            memo.current_status = "Cluster deleted".into();
            if let Err(e) = client.send_info(memo.clone()).await {
                error!(
                    "Cluster deleted but failed to send info message to k8s: {}",
                    e
                )
            }
        }

        info!("Done with cluster deletion");
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
