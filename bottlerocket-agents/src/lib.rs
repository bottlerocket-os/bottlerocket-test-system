pub mod error;
pub mod sonobuoy;

use crate::error::Error;
use crate::sonobuoy::Mode;
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
    pub mode: Mode,
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
pub fn init_agent_logger(bin_crate: &str, log_level: Option<LevelFilter>) {
    match env::var(env_logger::DEFAULT_FILTER_ENV).ok() {
        Some(_) => {
            // RUST_LOG exists; env_logger will use it.
            Builder::from_default_env().init();
        }
        None => {
            // RUST_LOG does not exist; use default log level except AWS SDK.
            let log_level = log_level.unwrap_or(DEFAULT_AGENT_LEVEL_FILTER);
            Builder::new()
                // Set log level to Error for crates other than our own.
                .filter_level(LevelFilter::Error)
                // Set all of our crates to the desired level.
                .filter(Some(bin_crate), log_level)
                .filter(Some("agent-common"), log_level)
                .filter(Some("bottlerocket-agents"), log_level)
                .filter(Some("model"), log_level)
                .filter(Some("resource-agent"), log_level)
                .filter(Some("test-agent"), log_level)
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
