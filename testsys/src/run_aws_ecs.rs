use crate::error::{self, Result};
use bottlerocket_agents::{
    ClusterType, Ec2Config, EcsClusterConfig, EcsTestConfig, AWS_CREDENTIALS_SECRET_NAME,
};
use kube::{api::ObjectMeta, Client};
use maplit::btreemap;
use model::clients::{CrdClient, ResourceClient, TestClient};
use model::constants::NAMESPACE;
use model::{
    Agent, Configuration, DestructionPolicy, Resource, ResourceSpec, SecretName, Test, TestSpec,
};
use snafu::ResultExt;
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
}

impl RunAwsEcs {
    pub(crate) async fn run(self, k8s_client: Client) -> Result<()> {
        let cluster_resource_name = self
            .cluster_resource_name
            .as_ref()
            .unwrap_or(&self.cluster_name);
        let ec2_resource_name = self
            .ec2_resource_name
            .unwrap_or(format!("{}-instances", self.cluster_name));
        let aws_secret_map = self.aws_secret.as_ref().map(|secret_name| {
            btreemap! [ AWS_CREDENTIALS_SECRET_NAME.to_string() => secret_name.clone()]
        });

        let ecs_resource = Resource {
            metadata: ObjectMeta {
                name: Some(cluster_resource_name.clone()),
                namespace: Some(NAMESPACE.into()),
                ..Default::default()
            },
            spec: ResourceSpec {
                depends_on: None,
                agent: Agent {
                    name: "ecs-provider".to_string(),
                    image: self.cluster_provider_image,
                    pull_secret: self.cluster_provider_pull_secret,
                    keep_running: false,
                    timeout: None,
                    configuration: Some(
                        EcsClusterConfig {
                            cluster_name: self.cluster_name.to_owned(),
                            region: Some(self.region.clone()),
                            vpc: self.vpc,
                        }
                        .into_map()
                        .context(error::ConfigMapSnafu)?,
                    ),
                    secrets: aws_secret_map.clone(),
                },
                ..Default::default()
            },
            status: None,
        };

        let ec2_config = Ec2Config {
            node_ami: self.ami,
            // TODO - configurable
            instance_count: Some(2),
            instance_type: self.instance_type,
            cluster_name: self.cluster_name.clone(),
            region: self.region,
            instance_profile_arn: self
                .iam_instance_profile_arn
                .unwrap_or_else(|| format!("${{{}.iamInstanceProfileArn}}", cluster_resource_name)),
            subnet_id: format!("${{{}.publicSubnetId}}", cluster_resource_name),
            cluster_type: ClusterType::Ecs,
            ..Default::default()
        }
        .into_map()
        .context(error::ConfigMapSnafu)?;

        let ec2_resource = Resource {
            metadata: ObjectMeta {
                name: Some(ec2_resource_name.clone()),
                namespace: Some(NAMESPACE.into()),
                ..Default::default()
            },
            spec: ResourceSpec {
                depends_on: Some(vec![cluster_resource_name.to_owned()]),
                agent: Agent {
                    name: "ec2-provider".to_string(),
                    image: self.ec2_provider_image,
                    pull_secret: self.ec2_provider_pull_secret,
                    keep_running: false,
                    timeout: None,
                    configuration: Some(ec2_config),
                    secrets: aws_secret_map.clone(),
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
                resources: vec![ec2_resource_name.clone(), cluster_resource_name.clone()],
                depends_on: Default::default(),
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
                                .task_definition_name_and_revision,
                        }
                        .into_map()
                        .context(error::ConfigMapSnafu)?,
                    ),
                    secrets: aws_secret_map,
                },
            },
            status: None,
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
