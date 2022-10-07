use crate::error::{self, Result};
use bottlerocket_types::agent_config::{
    ClusterType, CreationPolicy, Ec2Config, EksClusterConfig, K8sVersion, MigrationConfig,
    SonobuoyConfig, SonobuoyMode, TufRepoConfig, AWS_CREDENTIALS_SECRET_NAME,
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
use structopt::StructOpt;

/// Create an EKS resource, EC2 resource and run Sonobuoy.
#[derive(Debug, StructOpt)]
pub(crate) struct RunAwsK8s {
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

    /// The test mode passed to the sonobuoy E2E plugin. We default to `quick` to make a quick test
    /// the most ergonomic.
    #[structopt(long, default_value = "quick")]
    sonobuoy_mode: SonobuoyMode,

    /// Path to config file that overrides the registries for test images.
    /// Specifying this option passes the config to `sonobuoy run --e2e-repo-config`
    #[structopt(long)]
    sonobuoy_e2e_repo_config: Option<String>,

    /// The kubernetes conformance image used for the sonobuoy E2E plugin.
    #[structopt(long)]
    kubernetes_conformance_image: Option<String>,

    /// The name of the secret containing aws credentials.
    #[structopt(long)]
    aws_secret: Option<SecretName>,

    /// The AWS region.
    #[structopt(long, default_value = "us-west-2")]
    region: String,

    /// The name of the EKS cluster that will be used (whether it is being created or already
    /// exists).
    #[structopt(long)]
    cluster_name: String,

    /// The name of the TestSys resource that will represent this cluster. If you do not specify a
    /// value, one will be created matching the `cluster-name`. Unless there is a name conflict or
    /// you desire a specific resource name, then you do not need to supply a resource name here.
    #[structopt(long)]
    cluster_resource_name: Option<String>,

    /// The version of the EKS cluster that is to be created (with or without the 'v', e.g. 1.20 or
    /// v1.21, etc.) *This only affects EKS cluster creation!* If the cluster already exists, this
    /// option will have no affect.
    #[structopt(long)]
    cluster_version: Option<K8sVersion>,

    /// Whether or not we want the EKS cluster to be created. The possible values are:
    /// - `create`: the cluster will be created, it is an error for the cluster to pre-exist
    /// - `ifNotExists`: the cluster will be created if it does not already exist
    /// - `never`: the cluster must pre-exist or else it is an error
    #[structopt(long, default_value = "ifNotExists")]
    cluster_creation_policy: CreationPolicy,

    /// Whether or not we want the EKS cluster to be destroyed. The possible values are:
    /// - `onDeletion`: the cluster will be destroyed when its TestSys resource is deleted.
    /// - `never`: the cluster will not be destroyed.
    #[structopt(long, default_value = "never")]
    cluster_destruction_policy: DestructionPolicy,

    /// The container image of the EKS resource provider.
    // TODO - provide a default on ECR Public
    #[structopt(long)]
    cluster_provider_image: String,

    /// Name of the pull secret for the cluster provider image.
    #[structopt(long)]
    cluster_provider_pull_secret: Option<String>,

    /// Keep the EKS provider agent running after cluster creation.
    #[structopt(long)]
    keep_cluster_provider_running: bool,

    /// The EC2 AMI ID to use for cluster nodes.
    #[structopt(long)]
    ami: String,

    /// The EC2 instance type to use for cluster nodes. For example `m5.large`. If you do not
    /// provide an instance type, an appropriate instance type will be used based on the AMI's
    /// architecture.
    #[structopt(long)]
    instance_type: Option<String>,

    /// The name of the TestSys resource that will represent EC2 instances serving as cluster nodes.
    /// Defaults to `cluster-name-instances`.
    #[structopt(long)]
    ec2_resource_name: Option<String>,

    /// The container image of the EC2 resource provider.
    // TODO - provide a default on ECR Public
    #[structopt(long)]
    ec2_provider_image: String,

    /// Name of the pull secret for the EC2 provider image.
    #[structopt(long)]
    ec2_provider_pull_secret: Option<String>,

    /// Keep the EC2 instance provider running after instances are created.
    #[structopt(long)]
    keep_instance_provider_running: bool,

    /// Allow the sonobuoy test agent to rerun failed test.
    #[structopt(long)]
    retry_failed_attempts: Option<u32>,

    /// Perform an upgrade downgrade test.
    #[structopt(long, requires_all(&["starting-version", "upgrade-version", "migration-agent-image"]))]
    upgrade_downgrade: bool,

    /// Starting version for an upgrade/downgrade test.
    #[structopt(long, requires("upgrade-downgrade"))]
    starting_version: Option<String>,

    /// Version the ec2 instances should be upgraded to in an upgrade/downgrade test.
    #[structopt(long, requires("upgrade-downgrade"))]
    upgrade_version: Option<String>,

    /// Location of the tuf repo metadata.
    #[structopt(long, requires_all(&["tuf-repo-targets-url", "upgrade-downgrade"]))]
    tuf_repo_metadata_url: Option<String>,

    /// Location of the tuf repo targets.
    #[structopt(long, requires_all(&["tuf-repo-metadata-url", "upgrade-downgrade"]))]
    tuf_repo_targets_url: Option<String>,

    /// Location of the migration agent image.
    // TODO - default to an ECR public repository image
    #[structopt(long, short)]
    migration_agent_image: Option<String>,

    /// Name of the pull secret for the eks migration image (if needed).
    #[structopt(long)]
    migration_agent_pull_secret: Option<String>,

    /// The arn for the role that should be assumed by the agents.
    #[structopt(long)]
    assume_role: Option<String>,
}

impl RunAwsK8s {
    pub(crate) async fn run(self, k8s_client: Client) -> Result<()> {
        let cluster_resource_name = self
            .cluster_resource_name
            .as_ref()
            .unwrap_or(&self.cluster_name);
        let ec2_resource_name = self
            .ec2_resource_name
            .clone()
            .unwrap_or(format!("{}-instances", self.cluster_name));
        let aws_secret_map = self.aws_secret.as_ref().map(|secret_name| {
            btreemap! [ AWS_CREDENTIALS_SECRET_NAME.to_string() => secret_name.clone()]
        });

        let eks_resource = self.eks_resource(cluster_resource_name, aws_secret_map.clone())?;
        let ec2_resource = self.ec2_resource(
            &ec2_resource_name,
            aws_secret_map.clone(),
            cluster_resource_name,
        )?;

        let tests = if self.upgrade_downgrade {
            if let (Some(starting_version), Some(upgrade_version), Some(migration_agent_image)) = (
                self.starting_version.as_ref(),
                self.upgrade_version.as_ref(),
                self.migration_agent_image.as_ref(),
            ) {
                let tuf_repo = if let (Some(tuf_repo_metadata_url), Some(tuf_repo_targets_url)) = (
                    self.tuf_repo_metadata_url.as_ref(),
                    self.tuf_repo_targets_url.as_ref(),
                ) {
                    Some(TufRepoConfig {
                        metadata_url: tuf_repo_metadata_url.to_string(),
                        targets_url: tuf_repo_targets_url.to_string(),
                    })
                } else {
                    None
                };
                let init_test_name = format!("{}-1-initial", self.name);
                let upgrade_test_name = format!("{}-2-migrate", self.name);
                let upgraded_test_name = format!("{}-3-migrated", self.name);
                let downgrade_test_name = format!("{}-4-migrate", self.name);
                let final_test_name = format!("{}-5-final", self.name);
                let init_eks_test = self.sonobuoy_test(
                    &init_test_name,
                    aws_secret_map.clone(),
                    &ec2_resource_name,
                    cluster_resource_name,
                    None,
                )?;
                let upgrade_test = self.migration_test(
                    &upgrade_test_name,
                    upgrade_version,
                    tuf_repo.clone(),
                    aws_secret_map.clone(),
                    &ec2_resource_name,
                    cluster_resource_name,
                    Some(vec![init_test_name.clone()]),
                    migration_agent_image,
                    &self.migration_agent_pull_secret,
                )?;
                let upgraded_eks_test = self.sonobuoy_test(
                    &upgraded_test_name,
                    aws_secret_map.clone(),
                    &ec2_resource_name,
                    cluster_resource_name,
                    Some(vec![init_test_name.clone(), upgrade_test_name.clone()]),
                )?;
                let downgrade_test = self.migration_test(
                    &downgrade_test_name,
                    starting_version,
                    tuf_repo,
                    aws_secret_map.clone(),
                    &ec2_resource_name,
                    cluster_resource_name,
                    Some(vec![
                        init_test_name.clone(),
                        upgrade_test_name.clone(),
                        upgraded_test_name.clone(),
                    ]),
                    migration_agent_image,
                    &self.migration_agent_pull_secret,
                )?;
                let final_eks_test = self.sonobuoy_test(
                    &final_test_name,
                    aws_secret_map.clone(),
                    &ec2_resource_name,
                    cluster_resource_name,
                    Some(vec![
                        init_test_name,
                        upgrade_test_name,
                        upgraded_test_name,
                        downgrade_test_name,
                    ]),
                )?;
                vec![
                    init_eks_test,
                    upgrade_test,
                    upgraded_eks_test,
                    downgrade_test,
                    final_eks_test,
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
                aws_secret_map.clone(),
                &ec2_resource_name,
                cluster_resource_name,
                None,
            )?]
        };
        let resource_client = ResourceClient::new_from_k8s_client(k8s_client.clone());
        let test_client = TestClient::new_from_k8s_client(k8s_client);

        let _ = resource_client
            .create(eks_resource)
            .await
            .context(error::ModelClientSnafu {
                message: "Unable to create EKS cluster resource object",
            })?;
        println!("Created resource object '{}'", cluster_resource_name);

        let _ = resource_client
            .create(ec2_resource)
            .await
            .context(error::ModelClientSnafu {
                message: "Unable to create EC2 instances resource object",
            })?;
        println!("Created resource object '{}'", ec2_resource_name);

        for test in tests {
            let name = test.name_any();
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

    fn eks_resource(
        &self,
        name: &str,
        secrets: Option<BTreeMap<String, SecretName>>,
    ) -> Result<Resource> {
        Ok(Resource {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                namespace: Some(NAMESPACE.into()),
                ..Default::default()
            },
            spec: ResourceSpec {
                depends_on: None,
                conflicts_with: None,
                agent: Agent {
                    name: "eks-provider".to_string(),
                    image: self.cluster_provider_image.clone(),
                    pull_secret: self.cluster_provider_pull_secret.clone(),
                    keep_running: self.keep_cluster_provider_running,
                    timeout: None,
                    configuration: Some(
                        EksClusterConfig {
                            cluster_name: self.cluster_name.clone(),
                            creation_policy: Some(self.cluster_creation_policy),
                            region: Some(self.region.clone()),
                            zones: None,
                            version: self.cluster_version,
                            assume_role: self.assume_role.clone(),
                        }
                        .into_map()
                        .context(error::ConfigMapSnafu)?,
                    ),
                    secrets,
                    capabilities: None,
                },
                destruction_policy: self.cluster_destruction_policy,
            },
            status: None,
        })
    }

    fn ec2_resource(
        &self,
        name: &str,
        secrets: Option<BTreeMap<String, SecretName>>,
        cluster_resource_name: &str,
    ) -> Result<Resource> {
        let mut ec2_config = Ec2Config {
            node_ami: self.ami.clone(),
            // TODO - configurable
            instance_count: Some(2),
            instance_type: self.instance_type.clone(),
            cluster_name: self.cluster_name.clone(),
            region: self.region.clone(),
            instance_profile_arn: format!("${{{}.iamInstanceProfileArn}}", cluster_resource_name),
            subnet_ids: vec![],
            cluster_type: ClusterType::Eks,
            endpoint: Some(format!("${{{}.endpoint}}", cluster_resource_name)),
            certificate: Some(format!("${{{}.certificate}}", cluster_resource_name)),
            cluster_dns_ip: Some(format!("${{{}.clusterDnsIp}}", cluster_resource_name)),
            security_groups: vec![],
            assume_role: self.assume_role.clone(),
        }
        .into_map()
        .context(error::ConfigMapSnafu)?;

        // TODO - we have change the raw map to reference/template a non string field.
        let previous_value = ec2_config.insert(
            "securityGroups".to_owned(),
            Value::String(format!("${{{}.securityGroups}}", cluster_resource_name)),
        );
        if previous_value.is_none() {
            todo!("This is an error: fields in the Ec2Config struct have changed")
        }

        let previous_value = ec2_config.insert(
            "subnetIds".to_owned(),
            Value::String(format!("${{{}.privateSubnetIds}}", cluster_resource_name)),
        );
        if previous_value.is_none() {
            todo!("This is an error: fields in the Ec2Config struct have changed")
        }

        Ok(Resource {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                namespace: Some(NAMESPACE.into()),
                ..Default::default()
            },
            spec: ResourceSpec {
                depends_on: Some(vec![cluster_resource_name.to_owned()]),
                conflicts_with: None,
                agent: Agent {
                    name: "ec2-provider".to_string(),
                    image: self.ec2_provider_image.clone(),
                    pull_secret: self.ec2_provider_pull_secret.clone(),
                    keep_running: self.keep_instance_provider_running,
                    timeout: None,
                    configuration: Some(ec2_config),
                    secrets,
                    capabilities: None,
                },
                destruction_policy: DestructionPolicy::OnDeletion,
            },
            status: None,
        })
    }

    fn sonobuoy_test(
        &self,
        name: &str,
        secrets: Option<BTreeMap<String, SecretName>>,
        ec2_resource_name: &str,
        cluster_resource_name: &str,
        depends_on: Option<Vec<String>>,
    ) -> Result<Test> {
        let e2e_repo_config_string = match &self.sonobuoy_e2e_repo_config {
            Some(e2e_repo_config_path) => Some(base64::encode(
                read_to_string(e2e_repo_config_path).context(error::FileSnafu {
                    path: e2e_repo_config_path,
                })?,
            )),
            None => None,
        };

        Ok(Test {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                namespace: Some(NAMESPACE.into()),
                ..Default::default()
            },
            spec: TestSpec {
                resources: vec![
                    ec2_resource_name.to_string(),
                    cluster_resource_name.to_string(),
                ],
                depends_on,
                retries: self.retry_failed_attempts,
                agent: Agent {
                    name: "sonobuoy-test-agent".to_string(),
                    image: self.test_agent_image.clone(),
                    pull_secret: self.test_agent_pull_secret.clone(),
                    keep_running: self.keep_running,
                    timeout: None,
                    configuration: Some(
                        SonobuoyConfig {
                            kubeconfig_base64: format!(
                                "${{{}.encodedKubeconfig}}",
                                cluster_resource_name
                            ),
                            plugin: self.sonobuoy_plugin.clone(),
                            mode: self.sonobuoy_mode,
                            e2e_repo_config_base64: e2e_repo_config_string,
                            kubernetes_version: None,
                            kube_conformance_image: self.kubernetes_conformance_image.clone(),
                            assume_role: self.assume_role.clone(),
                        }
                        .into_map()
                        .context(error::ConfigMapSnafu)?,
                    ),
                    secrets,
                    // FIXME: Add CLI option for setting this
                    capabilities: None,
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
        secrets: Option<BTreeMap<String, SecretName>>,
        ec2_resource_name: &str,
        cluster_resource_name: &str,
        depends_on: Option<Vec<String>>,
        migration_agent_image: &str,
        migration_agent_pull_secret: &Option<String>,
    ) -> Result<Test> {
        let mut migration_config = MigrationConfig {
            aws_region: format!("${{{}.region}}", cluster_resource_name),
            instance_ids: Default::default(),
            migrate_to_version: version.to_string(),
            tuf_repo,
            assume_role: self.assume_role.clone(),
        }
        .into_map()
        .context(error::ConfigMapSnafu)?;
        migration_config.insert(
            "instanceIds".to_string(),
            Value::String(format!("${{{}.ids}}", ec2_resource_name)),
        );
        Ok(Test {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                namespace: Some(NAMESPACE.into()),
                ..Default::default()
            },
            spec: TestSpec {
                resources: vec![
                    ec2_resource_name.to_owned(),
                    cluster_resource_name.to_owned(),
                ],
                depends_on,
                retries: None,
                agent: Agent {
                    name: "eks-test-agent".to_string(),
                    image: migration_agent_image.to_string(),
                    pull_secret: migration_agent_pull_secret.clone(),
                    keep_running: self.keep_running,
                    timeout: None,
                    configuration: Some(migration_config),
                    secrets,
                    capabilities: None,
                },
            },
            status: None,
        })
    }
}
