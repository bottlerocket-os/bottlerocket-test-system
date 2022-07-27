use crate::error::{self, Result};
use bottlerocket_types::agent_config::{
    ClusterType, Ec2Config, EcsClusterConfig, EcsTestConfig, MigrationConfig, TufRepoConfig,
    AWS_CREDENTIALS_SECRET_NAME,
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
use structopt::StructOpt;

/// Create an EKS resource, EC2 resource and run Sonobuoy.
#[derive(Debug, StructOpt)]
pub(crate) struct RunAwsEcs {
    /// Name of the ecs test agent.
    #[structopt(long, short)]
    name: String,

    /// Location of the ecs test agent image.
    // TODO - default to an ECR public repository image
    #[structopt(long, short)]
    test_agent_image: String,

    /// Name of the pull secret for the ecs test image (if needed).
    #[structopt(long)]
    test_agent_pull_secret: Option<String>,

    /// Keep the test agent running after completion.
    #[structopt(long)]
    keep_running: bool,

    /// A specific task definition that the ecs test agent will use. If one isn't provided,
    /// a simple default task will be created and used.
    #[structopt(long)]
    task_definition_name_and_revision: Option<String>,

    /// The number of tasks that should be run (default value is 1).
    #[structopt(long, default_value = "1")]
    task_count: i32,

    /// The vpc that will be used for the ecs cluster (defaults to the default vpc).
    #[structopt(long)]
    vpc: Option<String>,

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

    /// The aws arn for the instace profile that the ec2 instances should be launched using. If
    /// no arn is provided, a testsys provided iam instance profile will be used.
    #[structopt(long)]
    iam_instance_profile_arn: Option<String>,

    /// The container image of the ECS resource provider.
    // TODO - provide a default on ECR Public
    #[structopt(long)]
    cluster_provider_image: String,

    /// Name of the pull secret for the cluster provider image.
    #[structopt(long)]
    cluster_provider_pull_secret: Option<String>,

    /// Keep the ECS cluster provider agent running after cluster creation.
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

    /// Perform an upgrade downgrade test.
    #[structopt(long, requires_all(&["starting-version", "upgrade-version"]))]
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
    #[structopt(long)]
    migration_agent_image: Option<String>,

    /// Name of the pull secret for the ecs migration image (if needed).
    #[structopt(long)]
    migration_agent_pull_secret: Option<String>,

    /// The arn for the role that should be assumed by the agents.
    #[structopt(long)]
    assume_role: Option<String>,
}

impl RunAwsEcs {
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

        let ecs_resource = self.ecs_resource(cluster_resource_name, aws_secret_map.clone())?;

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
                let init_ecs_test = self.ecs_test(
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
                let upgraded_ecs_test = self.ecs_test(
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
                let final_ecs_test = self.ecs_test(
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
                    init_ecs_test,
                    upgrade_test,
                    upgraded_ecs_test,
                    downgrade_test,
                    final_ecs_test,
                ]
            } else {
                return Err(error::Error::InvalidArguments {
                    why: "If performing an upgrade/downgrade test, \
                        `starting-version`, `upgrade-version` must be provided."
                        .to_string(),
                });
            }
        } else {
            vec![self.ecs_test(
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
            .create(ecs_resource)
            .await
            .context(error::ModelClientSnafu {
                message: "Unable to create ECS cluster resource object",
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

    fn ecs_resource(
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
                    name: "ecs-provider".to_string(),
                    image: self.cluster_provider_image.clone(),
                    pull_secret: self.cluster_provider_pull_secret.clone(),
                    keep_running: self.keep_cluster_provider_running,
                    timeout: None,
                    configuration: Some(
                        EcsClusterConfig {
                            cluster_name: self.cluster_name.to_owned(),
                            region: Some(self.region.clone()),
                            vpc: self.vpc.clone(),
                            assume_role: self.assume_role.clone(),
                        }
                        .into_map()
                        .context(error::ConfigMapSnafu)?,
                    ),
                    secrets,
                    capabilities: None,
                },
                ..Default::default()
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
        let ec2_config = Ec2Config {
            node_ami: self.ami.clone(),
            // TODO - configurable
            instance_count: Some(2),
            instance_type: self.instance_type.clone(),
            cluster_name: self.cluster_name.clone(),
            region: self.region.clone(),
            instance_profile_arn: self
                .iam_instance_profile_arn
                .clone()
                .unwrap_or_else(|| format!("${{{}.iamInstanceProfileArn}}", cluster_resource_name)),
            subnet_id: format!("${{{}.publicSubnetId}}", cluster_resource_name),
            cluster_type: ClusterType::Ecs,
            assume_role: self.assume_role.clone(),
            ..Default::default()
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

    fn ecs_test(
        &self,
        name: &str,
        secrets: Option<BTreeMap<String, SecretName>>,
        ec2_resource_name: &str,
        cluster_resource_name: &str,
        depends_on: Option<Vec<String>>,
    ) -> Result<Test> {
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
                    name: "ecs-test-agent".to_string(),
                    image: self.test_agent_image.clone(),
                    pull_secret: self.test_agent_pull_secret.clone(),
                    keep_running: self.keep_running,
                    timeout: None,
                    configuration: Some(
                        EcsTestConfig {
                            region: Some(format!("${{{}.region}}", cluster_resource_name)),
                            cluster_name: format!("${{{}.clusterName}}", cluster_resource_name),
                            task_count: self.task_count,
                            subnet: format!("${{{}.publicSubnetId}}", cluster_resource_name),
                            task_definition_name_and_revision: self
                                .task_definition_name_and_revision
                                .clone(),
                            assume_role: self.assume_role.clone(),
                        }
                        .into_map()
                        .context(error::ConfigMapSnafu)?,
                    ),
                    secrets,
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
                    name: "ecs-test-agent".to_string(),
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
