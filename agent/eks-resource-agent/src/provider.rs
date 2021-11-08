use model::{Configuration, SecretName};
use resource_agent::clients::InfoClient;
use resource_agent::provider::{
    Create, Destroy, IntoProviderError, ProviderError, ProviderResult, Resources, Spec,
};
use serde::{Deserialize, Serialize};
use std::env::{self, temp_dir};
use std::process::Command;

/// The default region for the cluster.
const DEFAULT_REGION: &str = "us-west-2";

/// The configuration information for a eks instance provider.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct ClusterConfig {
    /// The name of the eks cluster to create.
    cluster_name: String,

    /// The AWS region to create the cluster. If no value is provided `us-west-2` will be used.
    region: Option<String>,

    /// The availablility zones. (e.g. us-west-2a,us-west-2b)
    zones: Option<Vec<String>>,
}

impl Configuration for ClusterConfig {}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProductionMemo {
    pub current_status: String,

    /// The name of the secret containing aws credentials.
    pub aws_secret_name: Option<SecretName>,

    /// The name of the cluster we created.
    pub cluster_name: Option<String>,

    // The region the cluster is in.
    pub region: Option<String>,
}

impl Configuration for ProductionMemo {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct CreatedCluster {
    /// The name of the cluster we created.
    pub cluster_name: String,

    /// The region the cluster is in.
    pub region: String,

    // Base64 encoded kubeconfig
    pub encoded_kubeconfig: String,
}

impl Configuration for CreatedCluster {}

pub struct EksCreator {}

#[async_trait::async_trait]
impl Create for EksCreator {
    type Info = ProductionMemo;
    type Request = ClusterConfig;
    type Resource = CreatedCluster;

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
            .context(Resources::Clear, "Unable to get info from client")?;

        let region = request
            .configuration
            .region
            .as_ref()
            .unwrap_or(&DEFAULT_REGION.to_string())
            .to_string();

        let cluster_name = request.configuration.cluster_name;

        // Write aws credentials if we need them so we can run eksctl
        if let Some(aws_secret_name) = request.secrets.get("aws-credentials") {
            setup_env(client, aws_secret_name, Resources::Clear).await?;
            memo.aws_secret_name = Some(aws_secret_name.clone());
        }

        memo.current_status = "Creating Cluster".into();
        client
            .send_info(memo.clone())
            .await
            .context(Resources::Clear, "Error sending cluster creation message")?;

        let kubeconfig_dir = temp_dir().join("kubeconfig.yaml");

        let status = Command::new("eksctl")
            .args([
                "create",
                "cluster",
                "-r",
                &region,
                "--zones",
                &request.configuration.zones.unwrap_or_default().join(","),
                "--set-kubeconfig-context=false",
                "--kubeconfig",
                kubeconfig_dir.to_str().context(
                    Resources::Clear,
                    format!("Unable to convert '{:?}' to string path", kubeconfig_dir),
                )?,
                "-n",
                &cluster_name,
                "--nodes",
                "0",
                "--managed=false",
            ])
            .status()
            .context(Resources::Clear, "Failed create cluster")?;

        if !status.success() {
            return Err(ProviderError::new_with_context(
                Resources::Clear,
                format!("Failed create cluster with status code {}", status),
            ));
        }

        let kubeconfig = std::fs::read_to_string(kubeconfig_dir)
            .context(Resources::Remaining, "Unable to read kubeconfig.")?;
        let encoded_kubeconfig = base64::encode(kubeconfig);

        let created_lot = CreatedCluster {
            cluster_name: cluster_name.clone(),
            region: region.clone(),
            encoded_kubeconfig,
        };

        memo.current_status = "Cluster Created".into();
        memo.cluster_name = Some(cluster_name);
        memo.region = Some(region);
        client.send_info(memo.clone()).await.context(
            Resources::Remaining,
            "Error sending cluster created message",
        )?;

        Ok(created_lot)
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

pub struct EksDestroyer {}

#[async_trait::async_trait]
impl Destroy for EksDestroyer {
    type Request = ClusterConfig;
    type Info = ProductionMemo;
    type Resource = CreatedCluster;

    async fn destroy<I>(
        &self,
        _request: Option<Spec<Self::Request>>,
        _resource: Option<Self::Resource>,
        client: &I,
    ) -> ProviderResult<()>
    where
        I: InfoClient,
    {
        let mut memo: ProductionMemo = client
            .get_info()
            .await
            .context(Resources::Remaining, "Unable to get info from client")?;

        if let Some(cluster_name) = &memo.cluster_name {
            // Write aws credentials if we need them so we can run eksctl
            if let Some(aws_secret_name) = &memo.aws_secret_name {
                setup_env(client, aws_secret_name, Resources::Remaining).await?;
            }
            let region = memo.clone().region.unwrap_or(DEFAULT_REGION.to_string());
            let status = Command::new("eksctl")
                .args(["delete", "cluster", "--name", &cluster_name, "-r", &region])
                .status()
                .context(Resources::Remaining, "Failed to run eksctl delete command")?;
            if !status.success() {
                return Err(ProviderError::new_with_context(
                    Resources::Orphaned,
                    format!("Failed to delete cluster with status code {}", status),
                ));
            }
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
