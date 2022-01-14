use crate::error::{self, Result};
use bottlerocket_agents::sonobuoy::Mode;
use bottlerocket_agents::{
    ClusterConfig, ClusterType, CreationPolicy, Ec2Config, K8sVersion, SonobuoyConfig,
    AWS_CREDENTIALS_SECRET_NAME,
};
use kube::{api::ObjectMeta, Client};
use maplit::btreemap;
use model::clients::{CrdClient, ResourceClient, TestClient};
use model::constants::{API_VERSION, NAMESPACE};
use model::{
    Agent, Configuration, DestructionPolicy, Resource, ResourceSpec, SecretName, Test, TestSpec,
};
use serde_json::Value;
use snafu::ResultExt;
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

impl RunAwsK8s {
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

        let eks_resource = Resource {
            api_version: API_VERSION.into(),
            kind: "Resource".to_string(),
            metadata: ObjectMeta {
                name: Some(cluster_resource_name.clone()),
                namespace: Some(NAMESPACE.into()),
                ..Default::default()
            },
            spec: ResourceSpec {
                depends_on: None,
                agent: Agent {
                    name: "eks-provider".to_string(),
                    image: self.cluster_provider_image,
                    pull_secret: self.cluster_provider_pull_secret,
                    keep_running: false,
                    timeout: None,
                    configuration: Some(
                        ClusterConfig {
                            cluster_name: self.cluster_name.to_owned(),
                            creation_policy: Some(self.cluster_creation_policy),
                            region: Some(self.region.clone()),
                            zones: None,
                            version: self.kubernetes_version.clone(),
                        }
                        .into_map()
                        .context(error::ConfigMap)?,
                    ),
                    secrets: aws_secret_map.clone(),
                },
                destruction_policy: self.cluster_destruction_policy,
            },
            status: None,
        };

        let mut ec2_config = Ec2Config {
            node_ami: self.ami,
            // TODO - configurable
            instance_count: Some(2),
            instance_type: self.instance_type,
            cluster_name: self.cluster_name.clone(),
            region: self.region,
            instance_profile_arn: format!("${{{}.iamInstanceProfileArn}}", cluster_resource_name),
            subnet_id: format!("${{{}.publicSubnetId}}", cluster_resource_name),
            cluster_type: ClusterType::Eks,
            endpoint: Some(format!("${{{}.endpoint}}", cluster_resource_name)),
            certificate: Some(format!("${{{}.certificate}}", cluster_resource_name)),
            security_groups: vec![],
        }
        .into_map()
        .context(error::ConfigMap)?;

        // TODO - we have change the raw map to reference/template a non string field.
        let previous_value = ec2_config.insert(
            "securityGroups".to_owned(),
            Value::String(format!("${{{}.securityGroups}}", cluster_resource_name)),
        );
        if previous_value.is_none() {
            todo!("This is an error: fields in the Ec2Config struct have changed")
        }

        let ec2_resource = Resource {
            api_version: API_VERSION.into(),
            kind: "Resource".to_string(),
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
            api_version: API_VERSION.into(),
            kind: "Test".to_string(),
            metadata: ObjectMeta {
                name: Some(self.name.clone()),
                namespace: Some(NAMESPACE.into()),
                ..Default::default()
            },
            spec: TestSpec {
                resources: vec![ec2_resource_name.clone(), cluster_resource_name.clone()],
                depends_on: Default::default(),
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
                            mode: self.sonobuoy_mode.clone(),
                            kubernetes_version: self.kubernetes_version,
                            kube_conformance_image: self.kubernetes_conformance_image.clone(),
                        }
                        .into_map()
                        .context(error::ConfigMap)?,
                    ),
                    secrets: aws_secret_map,
                },
            },
            status: None,
        };
        let resource_client = ResourceClient::new_from_k8s_client(k8s_client.clone());
        let test_client = TestClient::new_from_k8s_client(k8s_client);

        let _ = resource_client
            .create(eks_resource)
            .await
            .context(error::ModelClientError {
                message: "Unable to create EKS cluster resource object",
            })?;
        println!("Created resource object '{}'", cluster_resource_name);

        let _ = resource_client
            .create(ec2_resource)
            .await
            .context(error::ModelClientError {
                message: "Unable to create EC2 instances resource object",
            })?;
        println!("Created resource object '{}'", ec2_resource_name);

        let _ = test_client
            .create(test)
            .await
            .context(error::ModelClientError {
                message: "Unable to create test object",
            })?;
        println!("Created test object '{}'", self.name);

        Ok(())
    }
}
