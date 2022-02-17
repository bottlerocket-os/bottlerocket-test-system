/*!

`bottlerocket-agents` is a collection of test agent and resource agent implementations used to test
Bottlerocket instances.
This `lib.rs` provides code that is used by multiple agent binaries orused by the `testsys` CLI.

!*/

pub mod error;
pub mod sonobuoy;
pub mod wireguard;

use crate::error::Error;
use crate::sonobuoy::Mode;
use env_logger::Builder;
use log::{info, LevelFilter};
use model::{Configuration, SecretName};
use resource_agent::clients::InfoClient;
use resource_agent::provider::{ProviderError, ProviderResult, Resources};
use serde::{Deserialize, Serialize};
use serde_plain::{
    derive_deserialize_from_fromstr, derive_display_from_serialize,
    derive_fromstr_from_deserialize, derive_serialize_from_display,
};
use snafu::{OptionExt, ResultExt};
use std::collections::HashSet;
use std::fmt::{Display, Formatter};
use std::path::Path;
use std::process::Output;
use std::str::FromStr;
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
    pub kubernetes_version: Option<K8sVersion>,
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

/// The configuration information for a eks instance provider.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EksClusterConfig {
    /// The name of the eks cluster to create or an existing cluster.
    pub cluster_name: String,

    /// Whether this agent will create the cluster or not.
    pub creation_policy: Option<CreationPolicy>,

    /// The AWS region to create the cluster. If no value is provided `us-west-2` will be used.
    pub region: Option<String>,

    /// The availability zones. (e.g. us-west-2a,us-west-2b)
    pub zones: Option<Vec<String>>,

    /// The eks version of the the cluster (e.g. "1.14", "1.15", "1.16"). Make sure this is
    /// quoted so that it is interpreted as a JSON/YAML string (not a number).
    pub version: Option<K8sVersion>,
}

impl Configuration for EksClusterConfig {}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CreationPolicy {
    /// Create the item, it is an error if the item already exists. This is the default
    /// behavior when no `CreationPolicy` is provided.
    Create,
    /// Create the item if it does not already exist.
    IfNotExists,
    /// Never create the item, it is an error if it does not exist.
    Never,
}

impl Default for CreationPolicy {
    fn default() -> Self {
        Self::Create
    }
}

derive_display_from_serialize!(CreationPolicy);
derive_fromstr_from_deserialize!(CreationPolicy);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Ec2Config {
    /// The AMI ID of the AMI to use for the worker nodes.
    pub node_ami: String,

    /// The number of instances to create. If no value is provided 2 instances will be created.
    pub instance_count: Option<i32>,

    /// The type of instance to spin up. m5.large is recommended for x86_64 and m6g.large is
    /// recommended for arm64 on eks. c3.large is recommended for ecs. If no value is provided
    /// the recommended type will be used.
    pub instance_type: Option<String>,

    /// The name of the cluster we are creating instances for.
    pub cluster_name: String,

    /// The region the cluster is located in.
    pub region: String,

    /// The instance profile that should be attached to these instances.
    pub instance_profile_arn: String,

    /// The subnet the instances should be launched using.
    pub subnet_id: String,

    /// The type of cluster we are launching instances to.
    pub cluster_type: ClusterType,

    // Userdata related fields.
    /// The eks server endpoint. The endpoint is required for eks clusters.
    pub endpoint: Option<String>,

    /// The eks certificate. The certificate is required for eks clusters.
    pub certificate: Option<String>,

    /// The cluster DNS IP for the K8s cluster. This is used to determine the IP family of the node IP.
    pub cluster_dns_ip: Option<String>,

    // Eks specific instance information.
    /// The security groups that should be attached to the instances.
    #[serde(default)]
    pub security_groups: Vec<String>,
}

impl Configuration for Ec2Config {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClusterType {
    Eks,
    Ecs,
}

impl Default for ClusterType {
    fn default() -> Self {
        Self::Eks
    }
}

/// The configuration information for an ecs instance provider.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EcsClusterConfig {
    /// The name of the ecs cluster to create.
    pub cluster_name: String,

    /// The AWS region to create the cluster. If no value is provided `us-west-2` will be used.
    pub region: Option<String>,

    /// The vpc to use for this clusters subnet ids. If no value is provided the default vpc will be used.
    pub vpc: Option<String>,
}

impl Configuration for EcsClusterConfig {}

/// Represents a parsed Kubernetes version. Examples of valid values when parsing:
/// - `v1.21`
/// - `1.21`
/// - `v1.21.1`
/// - `1.21.1`
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct K8sVersion {
    major: u8,
    minor: u8,
    patch: Option<u8>,
}

impl K8sVersion {
    pub const fn new(major: u8, minor: u8, patch: Option<u8>) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Returns a string representation of the Kubernetes version with a v prefix, and only includes
    /// the major and minor versions (event if a patch value is present). Example: `v1.21`.
    pub fn major_minor_with_v(&self) -> String {
        format!("v{}.{}", self.major, self.minor)
    }

    /// Returns a string representation of the Kubernetes version without a v prefix, and only
    /// includes the major and minor versions (event if a patch value is present). Example: `1.21`.
    pub fn major_minor_without_v(&self) -> String {
        format!("{}.{}", self.major, self.minor)
    }

    /// Returns a string representation of the Kubernetes version with a v prefix. Includes the
    /// patch value if it exists. Examples: `v1.21.1` when a patch value exists, or `v1.21` if the
    /// patch value is `None`.
    pub fn full_version_with_v(&self) -> String {
        if let Some(patch) = self.patch {
            format!("v{}.{}.{}", self.major, self.minor, patch)
        } else {
            self.major_minor_with_v()
        }
    }

    /// Returns a string representation of the Kubernetes version without a v prefix. Includes the
    /// patch value if it exists. Examples: `1.21.1` when a patch value exists, or `1.21` if the
    /// patch value is `None`.
    pub fn full_version_without_v(&self) -> String {
        if let Some(patch) = self.patch {
            format!("{}.{}.{}", self.major, self.minor, patch)
        } else {
            self.major_minor_without_v()
        }
    }

    pub fn parse<S: AsRef<str>>(s: S) -> std::result::Result<Self, String> {
        let original = s.as_ref();
        // skip the 'v' if present
        let no_v = if let Some(stripped) = original.strip_prefix('v') {
            stripped
        } else {
            original
        };
        let mut iter = no_v.split('.');
        let major = iter
            .next()
            .ok_or_else(|| {
                format!(
                    "Unable to find the major version number when parsing '{}' as a k8s version",
                    original
                )
            })?
            .parse::<u8>()
            .map_err(|e| {
                format!(
                    "Error when parsing the major version number of a k8s version: {}",
                    e
                )
            })?;
        let minor = iter
            .next()
            .ok_or_else(|| {
                format!(
                    "Unable to find the minor version number when parsing '{}' as a k8s version",
                    original
                )
            })?
            .parse::<u8>()
            .map_err(|e| {
                format!(
                    "Error when parsing the minor version number of a k8s version: {}",
                    e
                )
            })?;
        let patch = iter.next().and_then(|s| s.parse::<u8>().ok());
        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

impl Display for K8sVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.full_version_with_v(), f)
    }
}

impl FromStr for K8sVersion {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        K8sVersion::parse(s)
    }
}

derive_serialize_from_display!(K8sVersion);
derive_deserialize_from_fromstr!(K8sVersion, "k8s version such as v1.21 or 1.21.1");
pub const DEFAULT_TASK_DEFINITION: &str = "testsys-bottlerocket-aws-default-ecs-smoke-test-v1";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EcsTestConfig {
    pub region: Option<String>,
    pub cluster_name: String,
    #[serde(default = "default_count")]
    pub task_count: i32,
    pub subnet: String,
    /// The task definition (including the revision number) for a custom task to be run. If the task
    /// name is `foo` and the revision is `3`, use `foo:3`. If no
    /// `task_definition_name_and_revision` is provided, the agent will use the latest task
    /// definition named `testsys-bottlerocket-aws-default-ecs-smoke-test-v1` or create a new task
    /// definition by that name if it hasn't been created yet.
    pub task_definition_name_and_revision: Option<String>,
}

fn default_count() -> i32 {
    1
}

impl Configuration for EcsTestConfig {}

/// Decode and write out the kubeconfig file for a test cluster to a specified path
pub async fn decode_write_kubeconfig(
    kubeconfig_base64: &str,
    kubeconfig_path: &str,
) -> Result<(), error::Error> {
    let kubeconfig_path = Path::new(kubeconfig_path);
    info!("Decoding kubeconfig for test cluster");
    let decoded_bytes = base64::decode(kubeconfig_base64.as_bytes())
        .context(error::Base64DecodeSnafu { what: "kubeconfig" })?;
    info!("Storing kubeconfig in {}", kubeconfig_path.display());
    fs::write(kubeconfig_path, decoded_bytes).context(error::WriteSnafu { what: "kubeconfig" })?;
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

/// Set up AWS credential secrets in a runner's process's environment
pub async fn setup_test_env<R>(runner: &R, aws_secret_name: &SecretName) -> Result<(), R::E>
where
    R: Runner,
    <R as Runner>::E: From<Error>,
{
    let aws_secret = runner
        .get_secret(aws_secret_name)
        .context(error::SecretMissingSnafu)?;

    let access_key_id = String::from_utf8(
        aws_secret
            .get("access-key-id")
            .context(error::EnvSetupSnafu {
                what: format!("access-key-id missing from secret '{}'", aws_secret_name),
            })?
            .to_owned(),
    )
    .context(error::ConversionSnafu {
        what: "access-key-id",
    })?;
    let secret_access_key = String::from_utf8(
        aws_secret
            .get("secret-access-key")
            .context(error::EnvSetupSnafu {
                what: format!(
                    "secret-access-key missing from secret '{}'",
                    aws_secret_name
                ),
            })?
            .to_owned(),
    )
    .context(error::ConversionSnafu {
        what: "access-key-id",
    })?;

    env::set_var("AWS_ACCESS_KEY_ID", access_key_id);
    env::set_var("AWS_SECRET_ACCESS_KEY", secret_access_key);

    Ok(())
}

/// Print a value using `serde_json` `to_string_pretty` for types that implement Serialize.
pub fn json_display<T: Serialize>(object: T) -> String {
    serde_json::to_string_pretty(&object).unwrap_or_else(|e| format!("Serialization failed: {}", e))
}

/// Implement `Display` using `serde_json` `to_string_pretty` for types that implement Serialize.
#[macro_export]
macro_rules! impl_display_as_json {
    ($i:ident) => {
        impl std::fmt::Display for $i {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let s = serde_json::to_string_pretty(self)
                    .unwrap_or_else(|e| format!("Serialization failed: {}", e));
                std::fmt::Display::fmt(&s, f)
            }
        }
    };
}

#[test]
fn k8s_version_invalid() {
    let input = "1.foo";
    assert!(K8sVersion::parse(input).is_err())
}

#[test]
fn k8s_version_valid() {
    let input = "v1.21.3";
    let k8s_version = K8sVersion::from_str(input).unwrap();
    assert_eq!("v1.21", k8s_version.major_minor_with_v());
    assert_eq!("1.21", k8s_version.major_minor_without_v());
    assert_eq!("v1.21.3", k8s_version.full_version_with_v());
    assert_eq!("1.21.3", k8s_version.full_version_without_v());
}

/// Set up AWS credential secrets in a resource's process's environment
pub async fn setup_resource_env<I>(
    client: &I,
    aws_secret_name: &SecretName,
    resources: Resources,
) -> ProviderResult<()>
where
    I: InfoClient,
{
    let aws_secret = resource_agent::provider::IntoProviderError::context(
        client.get_secret(aws_secret_name).await,
        resources,
        format!("Error getting secret '{}'", aws_secret_name),
    )?;

    let access_key_id = resource_agent::provider::IntoProviderError::context(
        String::from_utf8(
            resource_agent::provider::IntoProviderError::context(
                aws_secret.get("access-key-id"),
                resources,
                format!("access-key-id missing from secret '{}'", aws_secret_name),
            )?
            .to_owned(),
        ),
        resources,
        "Could not convert access-key-id to String",
    )?;
    let secret_access_key = resource_agent::provider::IntoProviderError::context(
        String::from_utf8(
            resource_agent::provider::IntoProviderError::context(
                aws_secret.get("secret-access-key"),
                resources,
                format!(
                    "secret-access-key missing from secret '{}'",
                    aws_secret_name
                ),
            )?
            .to_owned(),
        ),
        resources,
        "Could not convert secret-access-key to String",
    )?;

    env::set_var("AWS_ACCESS_KEY_ID", access_key_id);
    env::set_var("AWS_SECRET_ACCESS_KEY", secret_access_key);

    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VSphereVmConfig {
    /// The name of the OVA file used for the VSphere worker nodes.
    pub ova_name: String,

    /// TUF repository where the OVA file can be found
    pub tuf_repo: TufRepoConfig,

    /// The number of VMs to create. If no value is provided 2 VMs will be created.
    pub vm_count: Option<i32>,

    /// URL of the vCenter instance to connect to
    pub vcenter_host_url: String,

    /// vCenter datacenter
    pub vcenter_datacenter: String,

    /// vCenter datastore
    pub vcenter_datastore: String,

    /// vCenter network
    pub vcenter_network: String,

    /// vCenter resource pool
    pub vcenter_resource_pool: String,

    /// The workloads folder to create the K8s cluster control plane in.
    pub vcenter_workload_folder: String,

    /// vSphere cluster information
    pub cluster: VSphereClusterInfo,
}

impl Configuration for VSphereVmConfig {}

/// If the command was successful (exit code zero), returns the command's `stdout`. Otherwise
/// returns a provider error.
/// - `output`: the `Output` object from a `std::process::Command`
/// - `hint`: the command that was executed, e.g. `echo hello world`
/// - `resources`: whether or not resources will be leftover if this command failed
pub fn provider_error_for_cmd_output(
    output: Output,
    hint: &str,
    resources: Resources,
) -> ProviderResult<String> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.success() {
        Ok(stdout.to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let code = output.status.code().unwrap_or(-1);
        Err(ProviderError::new_with_context(
            resources,
            format!(
                "Error running '{}', exit code {}\nstderr:\n{}\nstdout:\n{}",
                hint, code, stderr, stdout
            ),
        ))
    }
}
