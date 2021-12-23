pub mod error;
pub mod sonobuoy;

use crate::error::Error;
use env_logger::Builder;
use log::{info, LevelFilter};
use model::{Configuration, SecretName};
use serde::{Deserialize, Serialize};
use snafu::{OptionExt, ResultExt};
use std::collections::HashSet;
use std::path::Path;
use std::{env, fs};
use test_agent::Runner;

pub const AWS_CREDENTIALS_SECRET_NAME: &str = "awsCredentials";
pub const VSPHERE_CREDENTIALS_SECRET_NAME: &str = "vsphereCredentials";
pub const TEST_CLUSTER_KUBECONFIG_PATH: &str = "/local/test-cluster.kubeconfig";
pub const DEFAULT_AGENT_LEVEL_FILTER: LevelFilter = LevelFilter::Info;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClusterInfo {
    pub name: String,
    pub region: String,
    pub iam_instance_profile_arn: String,
    #[serde(default)]
    pub public_subnet_ids: Vec<String>,
    #[serde(default)]
    pub private_subnet_ids: Vec<String>,
    #[serde(default)]
    pub security_groups: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged, rename_all = "camelCase")]
pub enum UserData {
    Eks(EksUserData),
    Ecs(EcsUserData),
}

impl Default for UserData {
    fn default() -> Self {
        Self::Ecs(Default::default())
    }
}

impl UserData {
    pub fn user_data(&self, cluster_name: &str) -> String {
        match self {
            Self::Eks(eks) => eks.user_data(cluster_name),
            Self::Ecs(ecs) => ecs.user_data(cluster_name),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct EksUserData {
    pub certificate: String,
    pub endpoint: String,
}

impl EksUserData {
    pub fn user_data(&self, cluster_name: &str) -> String {
        base64::encode(format!(
            r#"[settings.updates]
ignore-waves = true
    
[settings.kubernetes]
api-server = "{}"
cluster-name = "{}"
cluster-certificate = "{}""#,
            self.endpoint, cluster_name, self.certificate
        ))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct EcsUserData {}

impl EcsUserData {
    pub fn user_data(&self, cluster_name: &str) -> String {
        base64::encode(format!(
            r#"[settings.ecs]
cluster-name = "{}""#,
            cluster_name
        ))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VSphereClusterInfo {
    pub name: String,
    pub control_plane_endpoint_ip: String,
    pub kubeconfig_base64: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SonobuoyConfig {
    // FIXME: need a better way of passing test cluster information
    pub kubeconfig_base64: String,
    pub plugin: String,
    pub mode: String,
    pub kubernetes_version: Option<String>,
    pub kube_conformance_image: Option<String>,
}

impl Configuration for SonobuoyConfig {}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TufRepoConfig {
    pub metadata_url: String,
    pub targets_url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationConfig {
    pub aws_region: String,
    pub instance_ids: HashSet<String>,
    pub migrate_to_version: String,
    pub tuf_repo: Option<TufRepoConfig>,
}

impl Configuration for MigrationConfig {}

/// Decode and write out the kubeconfig file for a test cluster to a specified path
pub async fn decode_write_kubeconfig(
    kubeconfig_base64: &str,
    kubeconfig_path: &str,
) -> Result<(), error::Error> {
    let kubeconfig_path = Path::new(kubeconfig_path);
    info!("Decoding kubeconfig for test cluster");
    let decoded_bytes =
        base64::decode(kubeconfig_base64.as_bytes()).context(error::Base64Decode)?;
    info!("Storing kubeconfig in {}", kubeconfig_path.display());
    fs::write(kubeconfig_path, decoded_bytes).context(error::KubeconfigWrite)?;
    Ok(())
}

/// Extract the value of `RUST_LOG` if it exists, otherwise log this application at
/// `DEFAULT_AGENT_LEVEL_FILTER`.
pub fn init_agent_logger() {
    match env::var(env_logger::DEFAULT_FILTER_ENV).ok() {
        Some(_) => {
            // RUST_LOG exists; env_logger will use it.
            Builder::from_default_env().init();
        }
        None => {
            // RUST_LOG does not exist; use default log level except AWS SDK.
            Builder::new()
                .filter_level(DEFAULT_AGENT_LEVEL_FILTER)
                .filter(Some("aws_"), LevelFilter::Error)
                .filter(Some("tracing"), LevelFilter::Error)
                .init();
        }
    }
}

/// Set up AWS credential secrets in the process's environment
pub async fn setup_env<R>(runner: &R, aws_secret_name: &SecretName) -> Result<(), R::E>
where
    R: Runner,
    <R as Runner>::E: From<Error>,
{
    let aws_secret = runner
        .get_secret(aws_secret_name)
        .context(error::SecretMissing)?;

    let access_key_id = String::from_utf8(
        aws_secret
            .get("access-key-id")
            .context(error::EnvSetup {
                what: format!("access-key-id missing from secret '{}'", aws_secret_name),
            })?
            .to_owned(),
    )
    .context(error::Conversion {
        what: "access-key-id",
    })?;
    let secret_access_key = String::from_utf8(
        aws_secret
            .get("secret-access-key")
            .context(error::EnvSetup {
                what: format!(
                    "secret-access-key missing from secret '{}'",
                    aws_secret_name
                ),
            })?
            .to_owned(),
    )
    .context(error::Conversion {
        what: "access-key-id",
    })?;

    env::set_var("AWS_ACCESS_KEY_ID", access_key_id);
    env::set_var("AWS_SECRET_ACCESS_KEY", secret_access_key);

    Ok(())
}
