use aws_config::meta::region::RegionProviderChain;
use aws_sdk_ec2::model::Filter;
use aws_sdk_ecs::Region;
use bottlerocket_agents::AWS_CREDENTIALS_SECRET_NAME;
use model::{Configuration, SecretName};
use resource_agent::clients::InfoClient;
use resource_agent::provider::{
    Create, Destroy, IntoProviderError, ProviderResult, Resources, Spec,
};
use serde::{Deserialize, Serialize};
use std::env;

/// The default region for the cluster.
const DEFAULT_REGION: &str = "us-west-2";

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

    // The region the cluster is in.
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

    /// A public subnet id for this cluster.
    pub public_subnet_id: Option<String>,

    /// A private subnet id for this cluster.
    pub private_subnet_id: Option<String>,
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

        // Write aws credentials if we need them so we can run Ecsctl
        if let Some(aws_secret_name) = spec.secrets.get(AWS_CREDENTIALS_SECRET_NAME) {
            setup_env(client, aws_secret_name, Resources::Clear).await?;
            memo.aws_secret_name = Some(aws_secret_name.clone());
        }

        let region_provider = RegionProviderChain::first_try(Some(Region::new(region.to_string())));
        let config = aws_config::from_env().region(region_provider).load().await;
        let ecs_client = aws_sdk_ecs::Client::new(&config);

        ecs_client
            .create_cluster()
            .cluster_name(&spec.configuration.cluster_name)
            .send()
            .await
            .context(Resources::Clear, "The cluster could not be created.")?;

        let created_cluster = created_cluster(
            &spec.configuration.cluster_name,
            region.clone(),
            spec.configuration.vpc,
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

async fn created_cluster(
    cluster_name: &str,
    region: String,
    vpc: Option<String>,
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
    })
}

fn first_subnet_id(subnet_ids: &[String]) -> Option<String> {
    subnet_ids.get(0).map(|id| id.to_string())
}

async fn setup_env<I>(
    client: &I,
    aws_secret_name: &SecretName,
    failure_resources: Resources,
) -> ProviderResult<()>
where
    I: InfoClient,
{
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

        if let Some(cluster_name) = &memo.cluster_name {
            // Write aws credentials if we need them so we can run Ecsctl
            if let Some(aws_secret_name) = &memo.aws_secret_name {
                setup_env(client, aws_secret_name, Resources::Remaining).await?;
            }
            let region = memo
                .clone()
                .region
                .unwrap_or_else(|| DEFAULT_REGION.to_string());

            let region_provider =
                RegionProviderChain::first_try(Some(Region::new(region.to_string())));
            let config = aws_config::from_env().region(region_provider).load().await;
            let ecs_client = aws_sdk_ecs::Client::new(&config);

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
