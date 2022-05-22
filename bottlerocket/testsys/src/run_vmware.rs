use crate::error::{self, Result};
use bottlerocket_types::agent_config::{
    MigrationConfig, SonobuoyConfig, SonobuoyMode, TufRepoConfig, VSphereClusterInfo,
    VSphereVmConfig, AWS_CREDENTIALS_SECRET_NAME, VSPHERE_CREDENTIALS_SECRET_NAME,
    WIREGUARD_SECRET_NAME,
};
use kube::ResourceExt;
use kube::{api::ObjectMeta, Client};
use maplit::btreemap;
use model::clients::{CrdClient, ResourceClient, TestClient};
use model::constants::NAMESPACE;
use model::{
    Agent, Configuration, DestructionPolicy, Resource, ResourceSpec, SecretName, Test, TestSpec,
};
use serde_json::Value;
use snafu::ResultExt;
use std::collections::BTreeMap;
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
    sonobuoy_mode: SonobuoyMode,

    /// The kubernetes conformance image used for the sonobuoy test.
    #[structopt(long)]
    kubernetes_conformance_image: Option<String>,

    /// The name of the secret containing aws credentials.
    #[structopt(long)]
    aws_secret: SecretName,

    /// The name of the Kubernetes secret containing vsphere credentials. This is required and will
    /// be used to authenticate with vCenter. This Kuberenetes secret should be a map containing the
    /// keys `username` and `password`.
    #[structopt(long)]
    vsphere_secret: SecretName,

    /// The name of the Kubernetes secret containing wireguard configuration. The given secret
    /// should be a map with the key `b64-wireguard-conf`. The value should be the base64 encoded
    /// representation of a wireguard conf file. Note that they will also need the `NET_ADMIN`
    /// capability provided by `--capabilities 'NET_ADMIN'` if you give a wireguard secret.
    #[structopt(long)]
    wireguard_secret: Option<SecretName>,

    /// The resource object name representing the vsphere cluster.
    #[structopt(long)]
    cluster_resource_name: String,

    /// The ova name to use for cluster nodes. This is the name of a file that will be found in the
    /// TUF repository provided by `--tuf-repo-metadata-url` and `--tuf-repo-targets-url`.
    #[structopt(long)]
    ova_name: String,

    /// The name of the TestSys resource that will represent the vms serving as cluster nodes.
    /// Defaults to `cluster-resource-name-vms`.
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

    /// Url for tuf repo metadata. The instance provider will get its OVA image from here.
    #[structopt(long)]
    tuf_repo_metadata_url: String,

    /// Url for tuf repo targets. The instance provider will get its OVA image from here.
    #[structopt(long)]
    tuf_repo_targets_url: String,

    /// URL of the vCenter instance to connect to.
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

    /// The ip for the cluster's control plane endpoint ip.
    #[structopt(long)]
    cluster_endpoint: String,

    /// Capabilities that should be enabled in the resource provider and the test agent. In order
    /// to enable wireguard in the VMWare test and resource agents, you should pass the `NET_ADMIN`
    /// capability. See Kuberenetes Security Context Capabilities for more information.
    #[structopt(long)]
    capabilities: Vec<String>,

    /// Keep the VMWare instance provider running after instances are created.
    #[structopt(long)]
    keep_instance_provider_running: bool,

    /// Allow the sonobuoy test agent to rerun failed test.
    #[structopt(long)]
    retry_failed_attempts: Option<u32>,

    /// Perform an upgrade downgrade test.
    #[structopt(long, requires_all(&["starting-version", "upgrade-version"]))]
    upgrade_downgrade: bool,

    /// Starting version for an upgrade/downgrade test.
    #[structopt(long, requires("upgrade-downgrade"))]
    starting_version: Option<String>,

    /// Version the ec2 instances should be upgraded to in an upgrade/downgrade test.
    #[structopt(long, requires("upgrade-downgrade"))]
    upgrade_version: Option<String>,

    /// The aws region for ssm on vm nodes
    #[structopt(long, default_value = "us-west-2")]
    region: String,

    /// Location of the migration agent image.
    // TODO - default to an ECR public repository image
    #[structopt(long)]
    migration_agent_image: Option<String>,

    /// Name of the pull secret for the ecs migration image (if needed).
    #[structopt(long)]
    migration_agent_pull_secret: Option<String>,

    /// The arn for the role that should be assumed by the agents.
    #[structopt(long)]
    assume_role: Option<String>,
}

impl RunVmware {
    pub(crate) async fn run(self, k8s_client: Client) -> Result<()> {
        let vm_resource_name = self
            .vm_resource_name
            .clone()
            .unwrap_or(format!("{}-vms", self.cluster_resource_name));
        let mut secret_map = btreemap![
            AWS_CREDENTIALS_SECRET_NAME.to_string() => self.aws_secret.clone(),
            VSPHERE_CREDENTIALS_SECRET_NAME.to_string() => self.vsphere_secret.clone()
        ];

        if let Some(wireguard_secret) = &self.wireguard_secret {
            secret_map.insert(WIREGUARD_SECRET_NAME.to_string(), wireguard_secret.clone());
        }

        let encoded_kubeconfig = base64::encode(
            read_to_string(&self.target_cluster_kubeconfig_path).context(error::FileSnafu {
                path: self.target_cluster_kubeconfig_path.clone(),
            })?,
        );

        let vm_resource = self.vm_resource(
            &vm_resource_name,
            Some(secret_map.clone()),
            &encoded_kubeconfig,
        )?;

        let tests = if self.upgrade_downgrade {
            if let (Some(starting_version), Some(upgrade_version), Some(migration_agent_image)) = (
                self.starting_version.as_ref(),
                self.upgrade_version.as_ref(),
                self.migration_agent_image.as_ref(),
            ) {
                let tuf_repo = TufRepoConfig {
                    metadata_url: self.tuf_repo_metadata_url.clone(),
                    targets_url: self.tuf_repo_targets_url.clone(),
                };
                let init_test_name = format!("{}-1-initial", self.name);
                let upgrade_test_name = format!("{}-2-migrate", self.name);
                let upgraded_test_name = format!("{}-3-migrated", self.name);
                let downgrade_test_name = format!("{}-4-migrate", self.name);
                let final_test_name = format!("{}-5-final", self.name);
                let init_sonobuoy_test = self.sonobuoy_test(
                    &init_test_name,
                    &encoded_kubeconfig,
                    secret_map.clone(),
                    &vm_resource_name,
                    None,
                )?;
                let upgrade_test = self.migration_test(
                    &upgrade_test_name,
                    upgrade_version,
                    Some(tuf_repo.clone()),
                    secret_map.clone(),
                    &vm_resource_name,
                    Some(vec![init_test_name.clone()]),
                    migration_agent_image,
                    &self.migration_agent_pull_secret,
                )?;
                let upgraded_sonobuoy_test = self.sonobuoy_test(
                    &upgraded_test_name,
                    &encoded_kubeconfig,
                    secret_map.clone(),
                    &vm_resource_name,
                    Some(vec![init_test_name.clone(), upgrade_test_name.clone()]),
                )?;
                let downgrade_test = self.migration_test(
                    &downgrade_test_name,
                    starting_version,
                    Some(tuf_repo),
                    secret_map.clone(),
                    &vm_resource_name,
                    Some(vec![
                        init_test_name.clone(),
                        upgrade_test_name.clone(),
                        upgraded_test_name.clone(),
                    ]),
                    migration_agent_image,
                    &self.migration_agent_pull_secret,
                )?;
                let final_sonobuoy_test = self.sonobuoy_test(
                    &final_test_name,
                    &encoded_kubeconfig,
                    secret_map.clone(),
                    &vm_resource_name,
                    Some(vec![
                        init_test_name,
                        upgrade_test_name,
                        upgraded_test_name,
                        downgrade_test_name,
                    ]),
                )?;
                vec![
                    init_sonobuoy_test,
                    upgrade_test,
                    upgraded_sonobuoy_test,
                    downgrade_test,
                    final_sonobuoy_test,
                ]
            } else {
                return Err(error::Error::InvalidArguments {
                    why: "If performing an upgrade/downgrade test,\
                         `starting-version`, `upgrade-version` must be provided."
                        .to_string(),
                });
            }
        } else {
            vec![self.sonobuoy_test(
                &self.name,
                &encoded_kubeconfig,
                secret_map.clone(),
                &vm_resource_name,
                None,
            )?]
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

        for test in tests {
            let name = test.name();
            let _ = test_client
                .create(test)
                .await
                .context(error::ModelClientSnafu {
                    message: "Unable to create test object",
                })?;
            println!("Created test object '{}'", name);
        }

        Ok(())
    }

    fn vm_resource(
        &self,
        name: &str,
        secrets: Option<BTreeMap<String, SecretName>>,
        encoded_kubeconfig: &str,
    ) -> Result<Resource> {
        let vm_config = VSphereVmConfig {
            ova_name: self.ova_name.clone(),
            vm_count: self.vm_count,
            tuf_repo: TufRepoConfig {
                metadata_url: self.tuf_repo_metadata_url.clone(),
                targets_url: self.tuf_repo_targets_url.clone(),
            },
            vcenter_host_url: self.vcenter_url.clone(),
            vcenter_datacenter: self.datacenter.clone(),
            vcenter_datastore: self.datastore.clone(),
            vcenter_network: self.network.clone(),
            vcenter_resource_pool: self.resource_pool.clone(),
            vcenter_workload_folder: self.workload_folder.clone(),
            cluster: VSphereClusterInfo {
                name: self.cluster_resource_name.clone(),
                control_plane_endpoint_ip: self.cluster_endpoint.clone(),
                kubeconfig_base64: encoded_kubeconfig.to_string(),
            },
            assume_role: self.assume_role.clone(),
        }
        .into_map()
        .context(error::ConfigMapSnafu)?;

        Ok(Resource {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                namespace: Some(NAMESPACE.into()),
                ..Default::default()
            },
            spec: ResourceSpec {
                depends_on: None,
                agent: Agent {
                    name: "vsphere-vm-provider".to_string(),
                    image: self.vm_provider_image.clone(),
                    pull_secret: self.vm_provider_pull_secret.clone(),
                    keep_running: self.keep_instance_provider_running,
                    timeout: None,
                    configuration: Some(vm_config),
                    secrets,
                    capabilities: Some(self.capabilities.clone()),
                },
                destruction_policy: DestructionPolicy::OnDeletion,
            },
            status: None,
        })
    }

    fn sonobuoy_test(
        &self,
        name: &str,
        encoded_kubeconfig: &str,
        secrets: BTreeMap<String, SecretName>,
        vm_resource_name: &str,
        depends_on: Option<Vec<String>>,
    ) -> Result<Test> {
        Ok(Test {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                namespace: Some(NAMESPACE.into()),
                ..Default::default()
            },
            spec: TestSpec {
                resources: vec![vm_resource_name.to_string()],
                depends_on,
                retries: self.retry_failed_attempts,
                agent: Agent {
                    name: "vmware-sonobuoy-test-agent".to_string(),
                    image: self.test_agent_image.clone(),
                    pull_secret: self.test_agent_pull_secret.clone(),
                    keep_running: self.keep_running,
                    timeout: None,
                    configuration: Some(
                        SonobuoyConfig {
                            kubeconfig_base64: encoded_kubeconfig.to_string(),
                            plugin: self.sonobuoy_plugin.clone(),
                            mode: self.sonobuoy_mode,
                            kubernetes_version: None,
                            kube_conformance_image: self.kubernetes_conformance_image.clone(),
                            assume_role: self.assume_role.clone(),
                        }
                        .into_map()
                        .context(error::ConfigMapSnafu)?,
                    ),
                    secrets: Some(secrets),
                    capabilities: Some(self.capabilities.clone()),
                },
            },
            status: None,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn migration_test(
        &self,
        name: &str,
        version: &str,
        tuf_repo: Option<TufRepoConfig>,
        secrets: BTreeMap<String, SecretName>,
        vm_resource_name: &str,
        depends_on: Option<Vec<String>>,
        migration_agent_image: &str,
        migration_agent_pull_secret: &Option<String>,
    ) -> Result<Test> {
        let mut migration_config = MigrationConfig {
            aws_region: self.region.to_string(),
            instance_ids: Default::default(),
            migrate_to_version: version.to_string(),
            tuf_repo,
            assume_role: self.assume_role.clone(),
        }
        .into_map()
        .context(error::ConfigMapSnafu)?;
        migration_config.insert(
            "instanceIds".to_string(),
            Value::String(format!("${{{}.instanceIds}}", vm_resource_name)),
        );
        Ok(Test {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                namespace: Some(NAMESPACE.into()),
                ..Default::default()
            },
            spec: TestSpec {
                resources: vec![vm_resource_name.to_owned()],
                depends_on,
                retries: None,
                agent: Agent {
                    name: "vmware-migration-test-agent".to_string(),
                    image: migration_agent_image.to_string(),
                    pull_secret: migration_agent_pull_secret.clone(),
                    keep_running: self.keep_running,
                    timeout: None,
                    configuration: Some(migration_config),
                    secrets: Some(secrets),
                    capabilities: Some(self.capabilities.clone()),
                },
            },
            status: None,
        })
    }
}
