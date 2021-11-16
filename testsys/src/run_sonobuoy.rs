use crate::error::{self, Result};
use kube::{api::ObjectMeta, Client};
use model::{
    clients::TestClient,
    constants::{API_VERSION, NAMESPACE},
    Agent, Configuration, SecretName, Test, TestSpec,
};
use snafu::ResultExt;
// use sonobuoy_test_agent::SonobuoyConfig;
use bottlerocket_agents::{SonobuoyConfig, SONOBUOY_AWS_SECRET_NAME};
use model::clients::CrdClient;
use std::{collections::BTreeMap, fs::read_to_string, path::PathBuf};
use structopt::StructOpt;

/// Run a test stored in a YAML file at `path`.
#[derive(Debug, StructOpt)]
pub(crate) struct RunSonobuoy {
    /// Path to test cluster's kubeconfig file.
    #[structopt(
        long,
        parse(from_os_str),
        required_if("target-cluster-kubeconfig", "None"),
        conflicts_with("target-cluster-kubeconfig")
    )]
    target_cluster_kubeconfig_path: Option<PathBuf>,

    /// The base64 encoded kubeconfig file for the target cluster, or a template such as
    /// `${mycluster.kubeconfig}`.
    #[structopt(long, required_if("target-cluster-kubeconfig-path", "None"))]
    target_cluster_kubeconfig: Option<String>,

    /// Name of the sonobuoy test.
    #[structopt(long, short)]
    name: String,

    /// Location of the sonobuoy test agent image.
    #[structopt(long, short)]
    image: String,

    /// Name of the pull secret for the sonobuoy test image (if needed).
    #[structopt(long)]
    pull_secret: Option<String>,

    /// Keep the test running after completion.
    #[structopt(long)]
    keep_running: bool,

    /// The plugin used for the sonobuoy test.
    #[structopt(long)]
    plugin: String,

    /// The mode used for the sonobuoy test.
    #[structopt(long)]
    mode: String,

    /// The kubernetes version used for the sonobuoy test.
    #[structopt(long)]
    kubernetes_version: Option<String>,

    /// The kubernetes conformance image used for the sonobuoy test.
    #[structopt(long)]
    kubernetes_conformance_image: Option<String>,

    /// The name of the secret containing aws credentials.
    #[structopt(long)]
    aws_secret: Option<SecretName>,

    /// The resources required by the sonobuoy test.
    #[structopt(long)]
    resource: Vec<String>,
}

impl RunSonobuoy {
    pub(crate) async fn run(&self, k8s_client: Client) -> Result<()> {
        let kubeconfig_string = match (&self.target_cluster_kubeconfig_path, &self.target_cluster_kubeconfig) {
            (Some(kubeconfig_path), None) => base64::encode(
                read_to_string(kubeconfig_path).context(error::File {
                    path: kubeconfig_path,
                })?,
            ),
            (None, Some(template_value)) => template_value.to_string(),
            (_, _) => return Err(error::Error::InvalidArguments { why: "Exactly 1 of 'target-cluster-kubeconfig' and 'target-cluster-kubeconfig-path' must be provided".to_string() })
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
                resources: self.resource.clone(),
                agent: Agent {
                    name: "sonobuoy-test-agent".to_string(),
                    image: self.image.clone(),
                    pull_secret: self.pull_secret.clone(),
                    keep_running: self.keep_running,
                    configuration: Some(
                        SonobuoyConfig {
                            kubeconfig_base64: kubeconfig_string,
                            plugin: self.plugin.clone(),
                            mode: self.mode.clone(),
                            kubernetes_version: self.kubernetes_version.clone(),
                            kube_conformance_image: self.kubernetes_conformance_image.clone(),
                        }
                        .into_map()
                        .context(error::ConfigMap)?,
                    ),
                    secrets: self.aws_secret.as_ref().map(|secret_name| {
                        let mut secrets_map = BTreeMap::new();
                        secrets_map
                            .insert(SONOBUOY_AWS_SECRET_NAME.to_string(), secret_name.clone());
                        secrets_map
                    }),
                },
            },
            status: None,
        };

        let tests = TestClient::new_from_k8s_client(k8s_client);

        tests.create(test).await.context(error::CreateTest)?;

        Ok(())
    }
}
