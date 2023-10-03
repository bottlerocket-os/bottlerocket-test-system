use builder_derive::Builder;
use configuration_derive::Configuration;
use serde::{Deserialize, Serialize};
use serde_plain::{
    derive_deserialize_from_fromstr, derive_display_from_serialize,
    derive_fromstr_from_deserialize, derive_serialize_from_display,
};
use std::collections::HashSet;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

pub const AWS_CREDENTIALS_SECRET_NAME: &str = "awsCredentials";
pub const SONOBUOY_RESULTS_FILENAME: &str = "sonobuoy-results.tar.gz";
pub const VSPHERE_CREDENTIALS_SECRET_NAME: &str = "vsphereCredentials";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VSphereK8sClusterInfo {
    pub name: String,
    pub control_plane_endpoint_ip: String,
    pub kubeconfig_base64: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CustomUserData {
    Replace { encoded_userdata: String },
    Merge { encoded_userdata: String },
}

/// What mode to run the e2e plugin in. Valid modes are `non-disruptive-conformance`,
/// `certified-conformance` and `quick`.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
// For most things we match Kubernetes style and use camelCase, but for this we want kebab case to
// match the format in which the argument is passed to Sonobuoy.
#[serde(rename_all = "kebab-case")]
pub enum SonobuoyMode {
    /// This is the default mode and will run all the tests in the e2e plugin which are marked
    /// `Conformance` which are known to not be disruptive to other workloads in your cluster.
    NonDisruptiveConformance,
    /// An unofficial mode of running the e2e tests which removes some of the longest running tests
    /// so that tests can complete in the fastest time possible while maximizing coverage.
    ConformanceLite,
    /// This mode runs all the tests in the K8s E2E conformance test suite.
    CertifiedConformance,
    /// This mode will run a single test from the e2e test suite which is known to be simple and
    /// fast. Use this mode as a quick check that the cluster is responding and reachable.
    Quick,
}

impl Default for SonobuoyMode {
    fn default() -> Self {
        Self::NonDisruptiveConformance
    }
}

derive_display_from_serialize!(SonobuoyMode);
derive_fromstr_from_deserialize!(SonobuoyMode);

#[derive(Debug, Clone, Default, Serialize, Deserialize, Configuration, Builder)]
#[serde(rename_all = "camelCase")]
#[crd("Test")]
pub struct SonobuoyConfig {
    pub kubeconfig_base64: String,
    pub plugin: String,
    pub mode: SonobuoyMode,
    pub e2e_repo_config_base64: Option<String>,
    /// This will be passed to `sonobuoy run` as `--kubernetes-version` if `kube_conformance_image`
    /// is `None`. **Caution**: if you provide `kubernetes_version`, it must precisely match the
    /// control plane version. If it is off by even a patch-level from the control plane, some tests
    /// may fail. Unless you have a specific reason to pass `kubernetes_version`, it is best to
    /// leave this as `None` and let the sonobuoy binary choose the right value.
    pub kubernetes_version: Option<K8sVersion>,
    pub kube_conformance_image: Option<String>,
    pub sonobuoy_image: Option<String>,
    pub assume_role: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TufRepoConfig {
    pub metadata_url: String,
    pub targets_url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Configuration, Builder)]
#[serde(rename_all = "camelCase")]
#[crd("Test")]
pub struct MigrationConfig {
    pub aws_region: String,
    pub instance_ids: HashSet<String>,
    pub migrate_to_version: String,
    pub tuf_repo: Option<TufRepoConfig>,
    pub assume_role: Option<String>,
}

/// The configuration information for a eks instance provider.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default, Configuration, Builder)]
#[serde(rename_all = "camelCase")]
#[crd("Resource")]
pub struct EksClusterConfig {
    /// Whether this agent will create the cluster or not.
    pub creation_policy: Option<CreationPolicy>,

    /// The role that should be assumed when creating the cluster.
    pub assume_role: Option<String>,

    /// The endpoint to create the EKS client with.
    /// This applicable when the cluster already exists at some EKS service endpoint, and we'd like
    /// to retrieve its details.
    pub eks_service_endpoint: Option<String>,

    #[serde(flatten)]
    pub config: EksctlConfig,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EksctlConfig {
    #[serde(rename_all = "camelCase")]
    File { encoded_config: String },
    #[serde(rename_all = "camelCase")]
    Args {
        cluster_name: String,
        region: Option<String>,
        /// The availability zones. (e.g. us-west-2a,us-west-2b)
        zones: Option<Vec<String>>,

        /// The eks version of the the cluster (e.g. "1.14", "1.15", "1.16"). Make sure this is
        /// quoted so that it is interpreted as a JSON/YAML string (not a number).
        version: Option<K8sVersion>,
    },
}

impl Default for EksctlConfig {
    fn default() -> Self {
        Self::File {
            encoded_config: "".to_string(),
        }
    }
}

/// The configuration information for a vSphere K8s cluster provider.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default, Configuration, Builder)]
#[serde(rename_all = "camelCase")]
#[crd("Resource")]
pub struct VSphereK8sClusterConfig {
    /// vSphere K8s cluster name.
    pub name: String,

    /// Cluster's control plane endpoint IP
    pub control_plane_endpoint_ip: String,

    /// Base64-encoded Kubeconfig for the K8s cluster if it already exists
    pub kubeconfig_base64: Option<String>,

    /// Whether this agent will create the cluster or not
    pub creation_policy: Option<CreationPolicy>,

    /// Version of the the K8s cluster (e.g. "1.22", "1.23")
    pub version: Option<K8sVersion>,

    /// Name of the OVA to download from TUF for creating the VMs to host cluster components
    pub ova_name: String,

    /// TUF repository where the OVA can be found
    pub tuf_repo: TufRepoConfig,

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

    /// Workloads folder to create the K8s cluster control plane in
    pub vcenter_workload_folder: String,

    /// URL for an EKS-A release manifest that contains URLs for EKS-A binary archives.
    /// Defaults to upstream EKS-A release channels.
    pub eks_a_release_manifest_url: Option<String>,

    /// Base64-encoded Kubeconfig for the CAPI management cluster
    pub mgmt_cluster_kubeconfig_base64: String,
}

/// The configuration information for a eks instance provider.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default, Configuration, Builder)]
#[serde(rename_all = "camelCase")]
#[crd("Resource")]
pub struct MetalK8sClusterConfig {
    /// URL for an EKS-A release manifest that contains URLs for EKS-A binary archives.
    /// Defaults to upstream EKS-A release channels.
    pub eks_a_release_manifest_url: Option<String>,

    /// Base64-encoded Kubeconfig for the CAPI management cluster
    pub mgmt_cluster_kubeconfig_base64: String,

    /// The role that should be assumed when activating SSM for the machines.
    pub assume_role: Option<String>,

    /// The base64-encoded EKS Anywhere config for this cluster.
    pub cluster_config_base64: String,

    /// The base64-encoded hardware csv that will be used for cluster creation.
    pub hardware_csv_base64: String,

    /// Custom TOML data that should be inserted into user-data settings.
    pub custom_user_data: Option<CustomUserData>,
}

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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default, Configuration, Builder)]
#[serde(rename_all = "camelCase")]
#[crd("Resource")]
pub struct Ec2Config {
    /// The AMI ID of the AMI to use for the worker nodes.
    pub node_ami: String,

    /// The number of instances to create. If no value is provided 2 instances will be created.
    pub instance_count: Option<i32>,

    /// The type of instance to spin up. m5.large is recommended for x86_64 and m6g.large is
    /// recommended for arm64 on eks. c3.large is recommended for ecs. If no value is provided
    /// the recommended type will be used.
    pub instance_types: Vec<String>,

    /// The name of the cluster we are creating instances for.
    pub cluster_name: String,

    /// The region the cluster is located in.
    pub region: String,

    /// The instance profile that should be attached to these instances.
    pub instance_profile_arn: String,

    /// The subnets the instances should be launched using.
    pub subnet_ids: Vec<String>,

    /// The type of cluster we are launching instances to.
    pub cluster_type: ClusterType,

    // Userdata related fields.
    /// The eks server endpoint. The endpoint is required for eks clusters.
    pub endpoint: Option<String>,

    /// The eks certificate. The certificate is required for eks clusters.
    pub certificate: Option<String>,

    /// The cluster DNS IP for the K8s cluster. This is used to determine the IP family of the node IP.
    pub cluster_dns_ip: Option<String>,

    /// The role that should be assumed when launching instances.
    pub assume_role: Option<String>,

    /// Custom TOML data that should be inserted into user-data settings.
    pub custom_user_data: Option<CustomUserData>,

    // Eks specific instance information.
    /// The security groups that should be attached to the instances.
    #[serde(default)]
    pub security_groups: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct KarpenterDeviceMapping {
    pub name: String,
    pub volume_type: String,
    pub volume_size: u8,
    pub delete_on_termination: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default, Configuration, Builder)]
#[serde(rename_all = "camelCase")]
#[crd("Resource")]
pub struct Ec2KarpenterConfig {
    /// The AMI ID of the AMI to use for the worker nodes.
    pub node_ami: String,

    /// The name of the cluster we are creating instances for.
    pub cluster_name: String,

    /// The region the cluster is located in.
    pub region: String,

    /// The subnets the instances should be launched using.
    pub subnet_ids: Vec<String>,

    /// The role that should be assumed when launching instances.
    pub assume_role: Option<String>,

    /// Custom TOML data that should be inserted into user-data settings.
    pub custom_user_data: Option<CustomUserData>,

    /// The eks server endpoint
    pub endpoint: String,

    /// The cluster security group
    pub cluster_sg: String,

    /// The device mappings used for karpenter provisioning
    #[serde(default)]
    pub device_mappings: Vec<KarpenterDeviceMapping>,

    /// The type of instance to spin up. m5.large is recommended for x86_64 and m6g.large is
    /// recommended for arm64 on eks
    #[serde(default)]
    pub instance_types: Vec<String>,
}

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
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default, Configuration, Builder)]
#[serde(rename_all = "camelCase")]
#[crd("Resource")]
pub struct EcsClusterConfig {
    /// The name of the ecs cluster to create.
    pub cluster_name: String,

    /// The AWS region to create the cluster. If no value is provided `us-west-2` will be used.
    pub region: Option<String>,

    /// The vpc to use for this clusters subnet ids. If no value is provided the default vpc will be used.
    pub vpc: Option<String>,

    /// The role that should be assumed when creating the ecs cluster.
    pub assume_role: Option<String>,

    /// The IAM instance profile name for the EC2 instances in the ECS cluster. If no value is
    /// provided, then the ECS test agent will attempt to create an IAM instance profile.
    pub iam_instance_profile_name: Option<String>,
}

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

#[derive(Debug, Clone, Default, Serialize, Deserialize, Configuration, Builder)]
#[serde(rename_all = "camelCase")]
#[crd("Test")]
pub struct EcsTestConfig {
    pub region: Option<String>,
    pub cluster_name: String,
    #[serde(default = "default_count")]
    pub task_count: i32,
    /// The task definition (including the revision number) for a custom task to be run. If the task
    /// name is `foo` and the revision is `3`, use `foo:3`. If no
    /// `task_definition_name_and_revision` is provided, the agent will use the latest task
    /// definition named `testsys-bottlerocket-aws-default-ecs-smoke-test-v1` or create a new task
    /// definition by that name if it hasn't been created yet.
    pub task_definition_name_and_revision: Option<String>,

    /// The role that should be assumed for this test agent.
    pub assume_role: Option<String>,
}

fn default_count() -> i32 {
    1
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, Configuration, Builder)]
#[serde(rename_all = "camelCase")]
#[crd("Resource")]
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
    pub cluster: VSphereK8sClusterInfo,

    /// The role that should be assumed when creating the vms.
    pub assume_role: Option<String>,

    /// Custom TOML data that should be inserted into user-data settings.
    pub custom_user_data: Option<CustomUserData>,
}

#[test]
fn k8s_version_invalid() {
    let input = "1.foo";
    assert!(K8sVersion::parse(input).is_err())
}

#[test]
fn k8s_version_valid() {
    use std::str::FromStr;
    let input = "v1.21.3";
    let k8s_version = K8sVersion::from_str(input).unwrap();
    assert_eq!("v1.21", k8s_version.major_minor_with_v());
    assert_eq!("1.21", k8s_version.major_minor_without_v());
    assert_eq!("v1.21.3", k8s_version.full_version_with_v());
    assert_eq!("1.21.3", k8s_version.full_version_without_v());
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkloadTest {
    pub name: String,
    pub image: String,
    #[serde(default)]
    pub gpu: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Configuration, Builder)]
#[serde(rename_all = "camelCase")]
#[crd("Test")]
pub struct WorkloadConfig {
    pub kubeconfig_base64: String,
    pub tests: Vec<WorkloadTest>,
    pub assume_role: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Configuration, Builder)]
#[serde(rename_all = "camelCase")]
#[crd("Test")]
pub struct EcsWorkloadTestConfig {
    pub region: Option<String>,
    pub cluster_name: String,
    /// The role that should be assumed for this test agent.
    pub assume_role: Option<String>,
    pub tests: Vec<WorkloadTest>,
}

#[cfg(test)]
mod test {
    use crate::agent_config::{Ec2Config, EcsClusterConfig, EcsTestConfig, MigrationConfig};
    use serde_json::Value as JsonValue;
    use std::fs::read_to_string;
    use std::path::PathBuf;
    use testsys_model::{Resource, Test};

    use super::{
        EcsWorkloadTestConfig, EksClusterConfig, MetalK8sClusterConfig, SonobuoyConfig,
        VSphereK8sClusterConfig, VSphereVmConfig, WorkloadConfig,
    };

    fn samples_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("samples")
    }

    fn read_eks_file(filename: &str) -> String {
        let p = samples_dir().join("eks").join(filename);
        read_to_string(&p).expect(&format!("unable to open '{}'", p.display()))
    }

    fn read_kind_file(filename: &str) -> String {
        let p = samples_dir().join("kind").join(filename);
        read_to_string(&p).expect(&format!("unable to open '{}'", p.display()))
    }

    // These tests assert that the sample configuration files can be deserialized into the agent
    // configuration structs.

    #[test]
    fn ecs_migration_test() {
        let s = read_eks_file("ecs-migration-test.yaml");
        let s = s
            .replace("\\${${CLUSTER_NAME}-instances.ids}", r#"["a", "b", "c"]"#)
            .replace("\\${${CLUSTER_NAME}.publicSubnetIds}", r#"["a", "b", "c"]"#)
            .replace("${", "<")
            .replace("}", ">");

        let docs: Vec<&str> = s.split("---").collect();
        let &yaml = docs.get(0).unwrap();
        let cluster_resource: Resource = serde_yaml::from_str(yaml).unwrap();
        let _: EcsClusterConfig = serde_json::from_value(JsonValue::Object(
            cluster_resource.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(1).unwrap();
        let ec2_resource: Resource = serde_yaml::from_str(yaml).unwrap();
        let _: Ec2Config = serde_json::from_value(JsonValue::Object(
            ec2_resource.spec.agent.configuration.unwrap(),
        ))
        .unwrap();
        let &yaml = docs.get(2).unwrap();
        let test_1_initial: Test = serde_yaml::from_str(yaml).unwrap();
        let _: EcsTestConfig = serde_json::from_value(JsonValue::Object(
            test_1_initial.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(3).unwrap();
        let test_2_migrate: Test = serde_yaml::from_str(yaml).unwrap();
        let _: MigrationConfig = serde_json::from_value(JsonValue::Object(
            test_2_migrate.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(4).unwrap();
        let test_3_migrated: Test = serde_yaml::from_str(yaml).unwrap();
        let _: EcsTestConfig = serde_json::from_value(JsonValue::Object(
            test_3_migrated.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(5).unwrap();
        let test_4_migrate: Test = serde_yaml::from_str(yaml).unwrap();
        let _: MigrationConfig = serde_json::from_value(JsonValue::Object(
            test_4_migrate.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(6).unwrap();
        let test_5_final: Test = serde_yaml::from_str(yaml).unwrap();
        let _: EcsTestConfig = serde_json::from_value(JsonValue::Object(
            test_5_final.spec.agent.configuration.unwrap(),
        ))
        .unwrap();
    }

    #[test]
    fn ecs_test() {
        let s = read_eks_file("ecs-test.yaml");
        let s = s
            .replace("\\${${CLUSTER_NAME}.publicSubnetIds}", r#"["a", "b", "c"]"#)
            .replace("${", "<")
            .replace("}", ">");

        let docs: Vec<&str> = s.split("---").collect();
        let &yaml = docs.get(0).unwrap();
        let cluster_resource: Resource = serde_yaml::from_str(yaml).unwrap();
        let _: EcsClusterConfig = serde_json::from_value(JsonValue::Object(
            cluster_resource.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(1).unwrap();
        let ec2_resource: Resource = serde_yaml::from_str(yaml).unwrap();
        let _: Ec2Config = serde_json::from_value(JsonValue::Object(
            ec2_resource.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(2).unwrap();
        let test_1_initial: Test = serde_yaml::from_str(yaml).unwrap();
        let _: EcsTestConfig = serde_json::from_value(JsonValue::Object(
            test_1_initial.spec.agent.configuration.unwrap(),
        ))
        .unwrap();
    }

    #[test]
    fn ecs_workload_test() {
        let s = read_eks_file("ecs-workload-test.yaml");
        let s = s
            .replace("\\${${CLUSTER_NAME}.publicSubnetIds}", r#"["a", "b", "c"]"#)
            .replace("${GPU}", "true")
            .replace("${INSTANCE_TYPES}", r#"["a", "b", "c"]"#)
            .replace("${", "<")
            .replace("}", ">");

        let docs: Vec<&str> = s.split("---").collect();
        let &yaml = docs.get(0).unwrap();
        let cluster_resource: Resource = serde_yaml::from_str(yaml).unwrap();
        let _: EcsClusterConfig = serde_json::from_value(JsonValue::Object(
            cluster_resource.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(1).unwrap();
        let ec2_resource: Resource = serde_yaml::from_str(yaml).unwrap();
        let _: Ec2Config = serde_json::from_value(JsonValue::Object(
            ec2_resource.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(2).unwrap();
        let test_1_initial: Test = serde_yaml::from_str(yaml).unwrap();
        let _: EcsWorkloadTestConfig = serde_json::from_value(JsonValue::Object(
            test_1_initial.spec.agent.configuration.unwrap(),
        ))
        .unwrap();
    }

    #[test]
    fn sonobuoy_migration_test() {
        let s = read_eks_file("sonobuoy-migration-test.yaml");
        let s = s
            .replace("\\${${CLUSTER_NAME}-instances.ids}", r#"["a", "b", "c"]"#)
            .replace("\\${${CLUSTER_NAME}.publicSubnetIds}", r#"["a", "b", "c"]"#)
            .replace("\\${${CLUSTER_NAME}.securityGroups}", r#"["a", "b", "c"]"#)
            .replace("${K8S_VERSION}", "v1.24")
            .replace("${", "<")
            .replace("}", ">");

        let docs: Vec<&str> = s.split("---").collect();
        let &yaml = docs.get(0).unwrap();
        let test_1_initial: Test = serde_yaml::from_str(yaml).unwrap();
        let _: SonobuoyConfig = serde_json::from_value(JsonValue::Object(
            test_1_initial.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(1).unwrap();
        let test_2_migrate: Test = serde_yaml::from_str(yaml).unwrap();
        let _: MigrationConfig = serde_json::from_value(JsonValue::Object(
            test_2_migrate.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(2).unwrap();
        let test_3_migrated: Test = serde_yaml::from_str(yaml).unwrap();
        let _: SonobuoyConfig = serde_json::from_value(JsonValue::Object(
            test_3_migrated.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(3).unwrap();
        let test_4_migrate: Test = serde_yaml::from_str(yaml).unwrap();
        let _: MigrationConfig = serde_json::from_value(JsonValue::Object(
            test_4_migrate.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(4).unwrap();
        let test_5_final: Test = serde_yaml::from_str(yaml).unwrap();
        let _: SonobuoyConfig = serde_json::from_value(JsonValue::Object(
            test_5_final.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(5).unwrap();
        let cluster_resource: Resource = serde_yaml::from_str(yaml).unwrap();
        let _: EksClusterConfig = serde_json::from_value(JsonValue::Object(
            cluster_resource.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(6).unwrap();
        let ec2_resource: Resource = serde_yaml::from_str(yaml).unwrap();
        let _: Ec2Config = serde_json::from_value(JsonValue::Object(
            ec2_resource.spec.agent.configuration.unwrap(),
        ))
        .unwrap();
    }

    #[test]
    fn sonobuoy_test() {
        let s = read_eks_file("sonobuoy-test.yaml");
        let s = s
            .replace("\\${${CLUSTER_NAME}.publicSubnetIds}", r#"["a", "b", "c"]"#)
            .replace("\\${${CLUSTER_NAME}.securityGroups}", r#"["a", "b", "c"]"#)
            .replace("${SONOBUOY_MODE}", "quick")
            .replace("${K8S_VERSION}", "v1.24")
            .replace("${", "<")
            .replace("}", ">");

        let docs: Vec<&str> = s.split("---").collect();
        let &yaml = docs.get(0).unwrap();
        let test_1_initial: Test = serde_yaml::from_str(yaml).unwrap();
        let _: SonobuoyConfig = serde_json::from_value(JsonValue::Object(
            test_1_initial.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(1).unwrap();
        let cluster_resource: Resource = serde_yaml::from_str(yaml).unwrap();
        let _: EksClusterConfig = serde_json::from_value(JsonValue::Object(
            cluster_resource.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(2).unwrap();
        let ec2_resource: Resource = serde_yaml::from_str(yaml).unwrap();
        let _: Ec2Config = serde_json::from_value(JsonValue::Object(
            ec2_resource.spec.agent.configuration.unwrap(),
        ))
        .unwrap();
    }

    #[test]
    fn k8s_workload_test() {
        let s = read_eks_file("k8s-workload-test.yaml");
        let s = s
            .replace("\\${${CLUSTER_NAME}.publicSubnetIds}", r#"["a", "b", "c"]"#)
            .replace("\\${${CLUSTER_NAME}.securityGroups}", r#"["a", "b", "c"]"#)
            .replace("${GPU}", "true")
            .replace("${K8S_VERSION}", "v1.24")
            .replace("${INSTANCE_TYPES}", r#"["a", "b", "c"]"#)
            .replace("${", "<")
            .replace("}", ">");

        let docs: Vec<&str> = s.split("---").collect();
        let &yaml = docs.get(0).unwrap();
        let test_1_initial: Test = serde_yaml::from_str(yaml).unwrap();
        let _: WorkloadConfig = serde_json::from_value(JsonValue::Object(
            test_1_initial.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(1).unwrap();
        let cluster_resource: Resource = serde_yaml::from_str(yaml).unwrap();
        let _: EksClusterConfig = serde_json::from_value(JsonValue::Object(
            cluster_resource.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(2).unwrap();
        let ec2_resource: Resource = serde_yaml::from_str(yaml).unwrap();
        let _: Ec2Config = serde_json::from_value(JsonValue::Object(
            ec2_resource.spec.agent.configuration.unwrap(),
        ))
        .unwrap();
    }

    #[test]
    fn vmware_migration_test() {
        let s = read_eks_file("vmware-migration-test.yaml");
        let s = s
            .replace("\\${${CLUSTER_NAME}-vms.instanceIds}", r#"["a", "b", "c"]"#)
            .replace("\\${${CLUSTER_NAME}.publicSubnetIds}", r#"["a", "b", "c"]"#)
            .replace("\\${${CLUSTER_NAME}.securityGroups}", r#"["a", "b", "c"]"#)
            .replace("${K8S_VERSION}", "v1.24")
            .replace("${", "<")
            .replace("}", ">")
            .replace("\\", "");

        let docs: Vec<&str> = s.split("---").collect();
        let &yaml = docs.get(0).unwrap();
        let test_1_initial: Test = serde_yaml::from_str(yaml).unwrap();
        let _: SonobuoyConfig = serde_json::from_value(JsonValue::Object(
            test_1_initial.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(1).unwrap();
        let test_2_migrate: Test = serde_yaml::from_str(yaml).unwrap();
        let _: MigrationConfig = serde_json::from_value(JsonValue::Object(
            test_2_migrate.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(2).unwrap();
        let test_3_migrated: Test = serde_yaml::from_str(yaml).unwrap();
        let _: SonobuoyConfig = serde_json::from_value(JsonValue::Object(
            test_3_migrated.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(3).unwrap();
        let test_4_migrate: Test = serde_yaml::from_str(yaml).unwrap();
        let _: MigrationConfig = serde_json::from_value(JsonValue::Object(
            test_4_migrate.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(4).unwrap();
        let test_5_final: Test = serde_yaml::from_str(yaml).unwrap();
        let _: SonobuoyConfig = serde_json::from_value(JsonValue::Object(
            test_5_final.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(5).unwrap();
        let cluster_resource: Resource = serde_yaml::from_str(yaml).unwrap();
        let _: VSphereK8sClusterConfig = serde_json::from_value(JsonValue::Object(
            cluster_resource.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(6).unwrap();
        let ec2_resource: Resource = serde_yaml::from_str(yaml).unwrap();
        let _: VSphereVmConfig = serde_json::from_value(JsonValue::Object(
            ec2_resource.spec.agent.configuration.unwrap(),
        ))
        .unwrap();
    }

    #[test]
    fn vmware_sonobuoy_test() {
        let s = read_eks_file("vmware-sonobuoy-test.yaml");
        let s = s
            .replace("\\${${CLUSTER_NAME}-vms.instanceIds}", r#"["a", "b", "c"]"#)
            .replace("\\${${CLUSTER_NAME}.publicSubnetIds}", r#"["a", "b", "c"]"#)
            .replace("\\${${CLUSTER_NAME}.securityGroups}", r#"["a", "b", "c"]"#)
            .replace("${K8S_VERSION}", "v1.24")
            .replace("${SONOBUOY_MODE}", "quick")
            .replace("${", "<")
            .replace("}", ">")
            .replace("\\", "");

        let docs: Vec<&str> = s.split("---").collect();
        let &yaml = docs.get(0).unwrap();
        let test_1_initial: Test = serde_yaml::from_str(yaml).unwrap();
        let _: SonobuoyConfig = serde_json::from_value(JsonValue::Object(
            test_1_initial.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(1).unwrap();
        let cluster_resource: Resource = serde_yaml::from_str(yaml).unwrap();
        let _: VSphereK8sClusterConfig = serde_json::from_value(JsonValue::Object(
            cluster_resource.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(2).unwrap();
        let ec2_resource: Resource = serde_yaml::from_str(yaml).unwrap();
        let _: VSphereVmConfig = serde_json::from_value(JsonValue::Object(
            ec2_resource.spec.agent.configuration.unwrap(),
        ))
        .unwrap();
    }

    #[test]
    fn metal_sonobuoy_test() {
        let s = read_eks_file("metal-sonobuoy-test.yaml");
        let s = s
            .replace("${SONOBUOY_MODE}", "quick")
            .replace("${", "<")
            .replace("}", ">")
            .replace("\\", "");

        let docs: Vec<&str> = s.split("---").collect();
        let &yaml = docs.get(0).unwrap();
        let test_1_initial: Test = serde_yaml::from_str(yaml).unwrap();
        let _: SonobuoyConfig = serde_json::from_value(JsonValue::Object(
            test_1_initial.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(1).unwrap();
        let cluster_resource: Resource = serde_yaml::from_str(yaml).unwrap();
        let _: MetalK8sClusterConfig = serde_json::from_value(JsonValue::Object(
            cluster_resource.spec.agent.configuration.unwrap(),
        ))
        .unwrap();
    }

    #[test]
    fn ecs_test_kind() {
        let s = read_kind_file("ecs-test.yaml");
        let s = s
            .replace("\\${${CLUSTER_NAME}.publicSubnetIds}", r#"["a", "b", "c"]"#)
            .replace("${", "<")
            .replace("}", ">");

        let docs: Vec<&str> = s.split("---").collect();
        let &yaml = docs.get(0).unwrap();
        let test_1_initial: Test = serde_yaml::from_str(yaml).unwrap();
        let _: EcsTestConfig = serde_json::from_value(JsonValue::Object(
            test_1_initial.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(1).unwrap();
        let cluster_resource: Resource = serde_yaml::from_str(yaml).unwrap();
        let _: EcsClusterConfig = serde_json::from_value(JsonValue::Object(
            cluster_resource.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(2).unwrap();
        let ec2_resource: Resource = serde_yaml::from_str(yaml).unwrap();
        let _: Ec2Config = serde_json::from_value(JsonValue::Object(
            ec2_resource.spec.agent.configuration.unwrap(),
        ))
        .unwrap();
    }

    #[test]
    fn ecs_workload_test_kind() {
        let s = read_kind_file("ecs-workload-test.yaml");
        let s = s
            .replace("\\${${CLUSTER_NAME}.publicSubnetIds}", r#"["a", "b", "c"]"#)
            .replace("${GPU}", "true")
            .replace("${INSTANCE_TYPES}", r#"["a", "b", "c"]"#)
            .replace("${", "<")
            .replace("}", ">");

        let docs: Vec<&str> = s.split("---").collect();
        let &yaml = docs.get(0).unwrap();
        let test_1_initial: Test = serde_yaml::from_str(yaml).unwrap();
        let _: EcsWorkloadTestConfig = serde_json::from_value(JsonValue::Object(
            test_1_initial.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(1).unwrap();
        let cluster_resource: Resource = serde_yaml::from_str(yaml).unwrap();
        let _: EcsClusterConfig = serde_json::from_value(JsonValue::Object(
            cluster_resource.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(2).unwrap();
        let ec2_resource: Resource = serde_yaml::from_str(yaml).unwrap();
        let _: Ec2Config = serde_json::from_value(JsonValue::Object(
            ec2_resource.spec.agent.configuration.unwrap(),
        ))
        .unwrap();
    }

    #[test]
    fn sonobuoy_test_kind() {
        let s = read_kind_file("sonobuoy-test.yaml");
        let s = s
            .replace("\\${${CLUSTER_NAME}.publicSubnetIds}", r#"["a", "b", "c"]"#)
            .replace("\\${${CLUSTER_NAME}.securityGroups}", r#"["a", "b", "c"]"#)
            .replace("${SONOBUOY_MODE}", "quick")
            .replace("${K8S_VERSION}", "v1.24")
            .replace("${", "<")
            .replace("}", ">");

        let docs: Vec<&str> = s.split("---").collect();
        let &yaml = docs.get(0).unwrap();
        let test_1_initial: Test = serde_yaml::from_str(yaml).unwrap();
        let _: SonobuoyConfig = serde_json::from_value(JsonValue::Object(
            test_1_initial.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(1).unwrap();
        let cluster_resource: Resource = serde_yaml::from_str(yaml).unwrap();
        let _: EksClusterConfig = serde_json::from_value(JsonValue::Object(
            cluster_resource.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(2).unwrap();
        let ec2_resource: Resource = serde_yaml::from_str(yaml).unwrap();
        let _: Ec2Config = serde_json::from_value(JsonValue::Object(
            ec2_resource.spec.agent.configuration.unwrap(),
        ))
        .unwrap();
    }

    #[test]
    fn vmware_sonobuoy_test_kind() {
        let s = read_kind_file("vmware-sonobuoy-test.yaml");
        let s = s
            .replace("\\${${CLUSTER_NAME}-vms.instanceIds}", r#"["a", "b", "c"]"#)
            .replace("\\${${CLUSTER_NAME}.publicSubnetIds}", r#"["a", "b", "c"]"#)
            .replace(
                "\\\\${${CLUSTER_NAME}.securityGroups}",
                r#"["a", "b", "c"]"#,
            )
            .replace("${K8S_VERSION}", "v1.24")
            .replace("${SONOBUOY_MODE}", "quick")
            .replace("${", "<")
            .replace("}", ">")
            .replace("\\", "");

        let docs: Vec<&str> = s.split("---").collect();
        let &yaml = docs.get(0).unwrap();
        let test_1_initial: Test = serde_yaml::from_str(yaml).unwrap();
        let _: SonobuoyConfig = serde_json::from_value(JsonValue::Object(
            test_1_initial.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(1).unwrap();
        let cluster_resource: Resource = serde_yaml::from_str(yaml).unwrap();
        let _: VSphereK8sClusterConfig = serde_json::from_value(JsonValue::Object(
            cluster_resource.spec.agent.configuration.unwrap(),
        ))
        .unwrap();

        let &yaml = docs.get(2).unwrap();
        let ec2_resource: Resource = serde_yaml::from_str(yaml).unwrap();
        let _: VSphereVmConfig = serde_json::from_value(JsonValue::Object(
            ec2_resource.spec.agent.configuration.unwrap(),
        ))
        .unwrap();
    }
}
