use crate::error::{self, Result};
use bottlerocket_types::agent_config::{
    ClusterType, CreationPolicy, CustomUserData, Ec2Config, EksClusterConfig, EksctlConfig,
    K8sVersion, MigrationConfig, SonobuoyConfig, SonobuoyMode, TufRepoConfig,
    AWS_CREDENTIALS_SECRET_NAME,
};
use kube::Client;
use kube::ResourceExt;
use maplit::btreemap;
use model::clients::{CrdClient, ResourceClient, TestClient};
use model::{DestructionPolicy, Resource, SecretName, Test};
use snafu::OptionExt;
use snafu::ResultExt;
use std::collections::BTreeMap;
use std::fs::read_to_string;
use std::str::FromStr;
use structopt::StructOpt;

#[derive(Clone, Debug)]
enum CustomUserDataMode {
    Merge,
    Replace,
}

impl FromStr for CustomUserDataMode {
    type Err = error::Error;
    fn from_str(custom_user_data_mode: &str) -> Result<Self> {
        match custom_user_data_mode {
            "merge" => Ok(CustomUserDataMode::Merge),
            "replace" => Ok(CustomUserDataMode::Replace),
            _ => Err(error::Error::InvalidArguments {
                why: "Invalid user data mode".to_string(),
            }),
        }
    }
}

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
    cluster_name: Option<String>,

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

    /// Path to eksctl config file.
    #[structopt(long)]
    cluster_config: Option<String>,

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

    /// The path to a TOML file containing custom userdata.
    #[structopt(long, requires("custom-user-data-mode"))]
    custom_user_data: Option<String>,

    /// The way custom userdata should interact with the default userdata.
    /// The possible values are `merge` and `replace`.
    #[structopt(long, requires("custom-user-data"))]
    custom_user_data_mode: Option<CustomUserDataMode>,
}

impl RunAwsK8s {
    pub(crate) async fn run(self, k8s_client: Client) -> Result<()> {
        let cluster_resource_name = self
            .cluster_resource_name
            .as_ref()
            .cloned()
            .or_else(|| self.cluster_name.as_ref().cloned())
            .unwrap_or_else(|| "cluster".to_string());
        let ec2_resource_name = self
            .ec2_resource_name
            .clone()
            .unwrap_or(format!("{}-instances", cluster_resource_name));
        let aws_secret_map = self.aws_secret.as_ref().map(|secret_name| {
            btreemap! [ AWS_CREDENTIALS_SECRET_NAME.to_string() => secret_name.clone()]
        });
        let eks_resource = self.eks_resource(&cluster_resource_name, aws_secret_map.clone())?;
        let ec2_resource = self.ec2_resource(
            &ec2_resource_name,
            aws_secret_map.clone(),
            &cluster_resource_name,
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
                    &cluster_resource_name,
                    None,
                )?;
                let upgrade_test = self.migration_test(
                    &upgrade_test_name,
                    upgrade_version,
                    tuf_repo.clone(),
                    aws_secret_map.clone(),
                    &ec2_resource_name,
                    &cluster_resource_name,
                    Some(vec![init_test_name.clone()]),
                    migration_agent_image,
                    &self.migration_agent_pull_secret,
                )?;
                let upgraded_eks_test = self.sonobuoy_test(
                    &upgraded_test_name,
                    aws_secret_map.clone(),
                    &ec2_resource_name,
                    &cluster_resource_name,
                    Some(vec![init_test_name.clone(), upgrade_test_name.clone()]),
                )?;
                let downgrade_test = self.migration_test(
                    &downgrade_test_name,
                    starting_version,
                    tuf_repo,
                    aws_secret_map.clone(),
                    &ec2_resource_name,
                    &cluster_resource_name,
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
                    &cluster_resource_name,
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
                &cluster_resource_name,
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
        let eksctl_config = if let Some(eksctl_config) = &self.cluster_config {
            EksctlConfig::File {
                encoded_config: base64::encode(read_to_string(eksctl_config).context(
                    error::FileSnafu {
                        path: eksctl_config,
                    },
                )?),
            }
        } else {
            EksctlConfig::Args {
                cluster_name: self.cluster_name.to_owned().context(
                    error::InvalidArgumentsSnafu {
                        why: "One of `cluster-name` or `cluster-config` must be provided.",
                    },
                )?,
                region: Some(self.region.clone()),
                zones: None,
                version: self.cluster_version,
            }
        };
        EksClusterConfig::builder()
            .image(&self.cluster_provider_image)
            .set_image_pull_secret(self.cluster_provider_pull_secret.clone())
            .creation_policy(self.cluster_creation_policy)
            .config(eksctl_config)
            .set_secrets(secrets)
            .destruction_policy(self.cluster_destruction_policy)
            .keep_running(self.keep_cluster_provider_running)
            .build(name)
            .context(error::BuildSnafu {
                what: name.to_string(),
            })
    }

    fn ec2_resource(
        &self,
        name: &str,
        secrets: Option<BTreeMap<String, SecretName>>,
        cluster_resource_name: &str,
    ) -> Result<Resource> {
        let user_data = &self
            .custom_user_data
            .clone()
            .map(read_to_string)
            .transpose()
            .context(error::ReadSnafu {})?
            .map(base64::encode);

        let user_data = match (self.custom_user_data_mode.clone(), user_data) {
            (Some(_), None) | (None, Some(_)) => return Err(error::Error::InvalidArguments { why: "Either both or neither of custom-user-data-mode and custom-user-data must be provided.".to_string() }),
            (Some(CustomUserDataMode::Merge), Some(userdata)) => Some(CustomUserData::Merge { encoded_userdata: userdata.to_owned() }),
            (Some(CustomUserDataMode::Replace), Some(userdata)) => Some(CustomUserData::Replace { encoded_userdata: userdata.to_owned() }),
            (None, None) => None
        };

        Ec2Config::builder()
            .image(&self.ec2_provider_image)
            .set_image_pull_secret(self.ec2_provider_pull_secret.clone())
            .node_ami(&self.ami)
            .instance_count(2)
            .instance_types(
                self.instance_type
                    .clone()
                    .map(|instance_type| vec![instance_type])
                    .unwrap_or_default(),
            )
            .cluster_name_template(cluster_resource_name, "clusterName")
            .region_template(cluster_resource_name, "region")
            .instance_profile_arn_template(cluster_resource_name, "iamInstanceProfileArn")
            .subnet_ids_template(cluster_resource_name, "privateSubnetIds")
            .cluster_type(ClusterType::Eks)
            .endpoint_template(cluster_resource_name, "endpoint")
            .certificate_template(cluster_resource_name, "certificate")
            .cluster_dns_ip_template(cluster_resource_name, "clusterDnsIp")
            .security_groups_template(cluster_resource_name, "securityGroups")
            .assume_role(self.assume_role.clone())
            .depends_on(cluster_resource_name)
            .set_secrets(secrets)
            .keep_running(self.keep_instance_provider_running)
            .destruction_policy(DestructionPolicy::OnDeletion)
            .custom_user_data(user_data)
            .build(name)
            .context(error::BuildSnafu {
                what: name.to_string(),
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

        SonobuoyConfig::builder()
            .resources(ec2_resource_name)
            .resources(cluster_resource_name)
            .set_depends_on(depends_on)
            .set_retries(self.retry_failed_attempts)
            .image(&self.test_agent_image)
            .set_image_pull_secret(self.test_agent_pull_secret.clone())
            .keep_running(self.keep_running)
            .kubeconfig_base64_template(cluster_resource_name, "encodedKubeconfig")
            .plugin(&self.sonobuoy_plugin)
            .mode(self.sonobuoy_mode)
            .e2e_repo_config_base64(e2e_repo_config_string)
            .kube_conformance_image(self.kubernetes_conformance_image.clone())
            .assume_role(self.assume_role.clone())
            .set_secrets(secrets)
            .build(name)
            .context(error::BuildSnafu {
                what: name.to_string(),
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
        MigrationConfig::builder()
            .aws_region_template(cluster_resource_name, "region")
            .instance_ids_template(ec2_resource_name, "instanceIds")
            .migrate_to_version(version)
            .tuf_repo(tuf_repo)
            .assume_role(self.assume_role.clone())
            .resources(ec2_resource_name)
            .resources(cluster_resource_name)
            .set_depends_on(depends_on)
            .image(migration_agent_image)
            .set_image_pull_secret(migration_agent_pull_secret.clone())
            .keep_running(self.keep_running)
            .set_secrets(secrets)
            .build(name)
            .context(error::BuildSnafu {
                what: name.to_string(),
            })
    }
}
