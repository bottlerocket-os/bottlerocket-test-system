use crate::error::{self, Result};
use bottlerocket_agents::sonobuoy::Mode;
use bottlerocket_agents::wireguard::WIREGUARD_SECRET_NAME;
use bottlerocket_agents::{
    K8sVersion, SonobuoyConfig, VSphereVmConfig, AWS_CREDENTIALS_SECRET_NAME,
    VSPHERE_CREDENTIALS_SECRET_NAME,
};
use kube::{api::ObjectMeta, Client};
use maplit::btreemap;
use model::clients::{CrdClient, ResourceClient, TestClient};
use model::constants::NAMESPACE;
use model::{
    Agent, Configuration, DestructionPolicy, Resource, ResourceSpec, SecretName, Test, TestSpec,
};
use snafu::ResultExt;
use std::fs::read_to_string;
use std::path::PathBuf;
use structopt::StructOpt;

/// Create vmware nodes and run Sonobuoy.
#[derive(Debug, StructOpt)]
pub(crate) struct RunVmware {
    /// Name of the sonobuoy test.
    #[structopt(long, short)]
    name: String,

    /// Location of the sonobuoy test agent image.
    // TODO - default to an ECR public repository image
    #[structopt(long, short)]
    test_agent_image: String,

    /// Name of the pull secret for the sonobuoy test image (if needed).
    #[structopt(long)]
    test_agent_pull_secret: Option<String>,

    /// Keep the test agent running after completion.
    #[structopt(long)]
    keep_running: bool,

    /// The plugin used for the sonobuoy test. Normally this is `e2e` (the default).
    #[structopt(long, default_value = "e2e")]
    sonobuoy_plugin: String,

    /// The mode used for the sonobuoy test. One of `non-disruptive-conformance`,
    /// `certified-conformance`, `quick`. Although the Sonobuoy binary defaults to
    /// `non-disruptive-conformance`, we default to `quick` to make a quick test the most ergonomic.
    #[structopt(long, default_value = "quick")]
    sonobuoy_mode: Mode,

    /// The kubernetes version (with or without the v prefix). Examples: v1.21, 1.21.3, v1.20.1
    #[structopt(long)]
    kubernetes_version: Option<K8sVersion>,

    /// The kubernetes conformance image used for the sonobuoy test.
    #[structopt(long)]
    kubernetes_conformance_image: Option<String>,

    /// The name of the secret containing aws credentials.
    #[structopt(long)]
    aws_secret: SecretName,

    /// The name of the secret containing vsphere credentials.
    #[structopt(long)]
    vsphere_secret: SecretName,

    /// The name of the secret containing wireguard configuration.
    #[structopt(long)]
    wireguard_secret: SecretName,

    /// The name of the vsphere cluster that will be used.
    #[structopt(long)]
    cluster_name: String,

    /// The ova name to use for cluster nodes.
    #[structopt(long)]
    ova_name: String,

    /// The name of the TestSys resource that will represent the vms serving as cluster nodes.
    /// Defaults to `cluster-name-vms`.
    #[structopt(long)]
    vm_resource_name: Option<String>,

    /// The container image of the VMWare resource provider.
    // TODO - provide a default on ECR Public
    #[structopt(long)]
    vm_provider_image: String,

    /// Name of the pull secret for the VMWare VM provider image.
    #[structopt(long)]
    vm_provider_pull_secret: Option<String>,

    /// The number of vm nodes to launch.
    #[structopt(long)]
    vm_count: Option<i32>,

    /// Url for tuf repo metadata.
    #[structopt(long)]
    tuf_repo_metadata_url: String,

    /// Url for tuf repo targets.
    #[structopt(long)]
    tuf_repo_targets_url: String,

    /// URL of the vCenter instance to connect to
    #[structopt(long)]
    vcenter_url: String,

    /// vCenter datacenter
    #[structopt(long, default_value = "SDDC-Datacenter")]
    datacenter: String,

    /// vCenter datastore
    #[structopt(long, default_value = "WorkloadDatastore")]
    datastore: String,

    /// vCenter network
    #[structopt(long, default_value = "sddc-cgw-network-2")]
    network: String,

    /// vCenter resource pool
    #[structopt(
        long,
        default_value = "/SDDC-Datacenter/host/Cluster-1/Resources/Compute-ResourcePool"
    )]
    resource_pool: String,

    /// The workloads folder to create the VMWare resources in.
    #[structopt(long, default_value = "testsys")]
    workload_folder: String,

    /// Path to test cluster's kubeconfig file.
    #[structopt(long, parse(from_os_str))]
    target_cluster_kubeconfig_path: PathBuf,

    /// The ip for the cluster's control plane enedpoint ip.
    #[structopt(long)]
    cluster_endpoint: String,

    /// Capabilities that should be enabled in the resource provider and the test agent.
    #[structopt(long)]
    capabilities: Vec<String>,
}

impl RunVmware {
    pub(crate) async fn run(self, k8s_client: Client) -> Result<()> {
        let vm_resource_name = self
            .vm_resource_name
            .unwrap_or(format!("{}-vms", self.cluster_name));
        let secret_map = btreemap! [ AWS_CREDENTIALS_SECRET_NAME.to_string() => self.aws_secret.clone(),
        VSPHERE_CREDENTIALS_SECRET_NAME.to_string() => self.vsphere_secret,
        WIREGUARD_SECRET_NAME.to_string() => self.wireguard_secret ];

        let encoded_kubeconfig = base64::encode(
            read_to_string(&self.target_cluster_kubeconfig_path).context(error::FileSnafu {
                path: self.target_cluster_kubeconfig_path,
            })?,
        );

        let vm_config = VSphereVmConfig {
            ova_name: self.ova_name,
            vm_count: self.vm_count,
            tuf_repo: bottlerocket_agents::TufRepoConfig {
                metadata_url: self.tuf_repo_metadata_url,
                targets_url: self.tuf_repo_targets_url,
            },
            vcenter_host_url: self.vcenter_url,
            vcenter_datacenter: self.datacenter,
            vcenter_datastore: self.datastore,
            vcenter_network: self.network,
            vcenter_resource_pool: self.resource_pool,
            vcenter_workload_folder: self.workload_folder,
            cluster: bottlerocket_agents::VSphereClusterInfo {
                name: self.cluster_name,
                control_plane_endpoint_ip: self.cluster_endpoint,
                kubeconfig_base64: encoded_kubeconfig.clone(),
            },
        }
        .into_map()
        .context(error::ConfigMapSnafu)?;

        let vm_resource = Resource {
            metadata: ObjectMeta {
                name: Some(vm_resource_name.clone()),
                namespace: Some(NAMESPACE.into()),
                ..Default::default()
            },
            spec: ResourceSpec {
                depends_on: None,
                agent: Agent {
                    name: "vsphere-vm-provider".to_string(),
                    image: self.vm_provider_image,
                    pull_secret: self.vm_provider_pull_secret,
                    keep_running: false,
                    timeout: None,
                    configuration: Some(vm_config),
                    secrets: Some(secret_map.clone()),
                    capabilities: Some(self.capabilities.clone()),
                },
                destruction_policy: DestructionPolicy::OnDeletion,
            },
            status: None,
        };

        let test = Test {
            metadata: ObjectMeta {
                name: Some(self.name.clone()),
                namespace: Some(NAMESPACE.into()),
                ..Default::default()
            },
            spec: TestSpec {
                resources: vec![vm_resource_name.clone()],
                depends_on: Default::default(),
                agent: Agent {
                    name: "vmware-sonobuoy-test-agent".to_string(),
                    image: self.test_agent_image.clone(),
                    pull_secret: self.test_agent_pull_secret.clone(),
                    keep_running: self.keep_running,
                    timeout: None,
                    configuration: Some(
                        SonobuoyConfig {
                            kubeconfig_base64: encoded_kubeconfig,
                            plugin: self.sonobuoy_plugin.clone(),
                            mode: self.sonobuoy_mode,
                            kubernetes_version: self.kubernetes_version,
                            kube_conformance_image: self.kubernetes_conformance_image.clone(),
                        }
                        .into_map()
                        .context(error::ConfigMapSnafu)?,
                    ),
                    secrets: Some(secret_map),
                    capabilities: Some(self.capabilities.clone()),
                },
            },
            status: None,
        };
        let resource_client = ResourceClient::new_from_k8s_client(k8s_client.clone());
        let test_client = TestClient::new_from_k8s_client(k8s_client);

        let _ = resource_client
            .create(vm_resource)
            .await
            .context(error::ModelClientSnafu {
                message: "Unable to create vm nodes resource object",
            })?;
        println!("Created resource object '{}'", vm_resource_name);

        let _ = test_client
            .create(test)
            .await
            .context(error::ModelClientSnafu {
                message: "Unable to create test object",
            })?;
        println!("Created test object '{}'", self.name);

        Ok(())
    }
}
