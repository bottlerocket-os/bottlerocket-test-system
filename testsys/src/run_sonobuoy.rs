use crate::error::{self, Result};
use kube::{api::ObjectMeta, Client};
use model::{
    clients::TestClient,
    constants::{API_VERSION, NAMESPACE},
    Agent, Configuration, Test, TestSpec,
};
use snafu::ResultExt;
// use sonobuoy_test_agent::SonobuoyConfig;
use sonobuoy_test_agent::SonobuoyConfig;
use std::{fs::read_to_string, path::PathBuf};
use structopt::StructOpt;

/// Run a test stored in a YAML file at `path`.
#[derive(Debug, StructOpt)]
pub(crate) struct RunSonobuoy {
    /// Path to test cluster's kubeconfig file.
    #[structopt(long, parse(from_os_str))]
    target_cluster_kubeconfig: PathBuf,

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
}

impl RunSonobuoy {
    pub(crate) async fn run(&self, k8s_client: Client) -> Result<()> {
        let kubeconfig_base64 = base64::encode(
            read_to_string(&self.target_cluster_kubeconfig).context(error::File {
                path: &self.target_cluster_kubeconfig,
            })?,
        );

        let test = Test {
            api_version: API_VERSION.into(),
            kind: "Test".to_string(),
            metadata: ObjectMeta {
                name: Some(self.name.clone()),
                namespace: Some(NAMESPACE.into()),
                ..Default::default()
            },
            spec: TestSpec {
                resources: Default::default(),
                agent: Agent {
                    name: "sonobuoy-test-agent".to_string(),
                    image: self.image.clone(),
                    pull_secret: self.pull_secret.clone(),
                    keep_running: self.keep_running,
                    configuration: Some(
                        SonobuoyConfig {
                            kubeconfig_base64,
                            plugin: self.plugin.clone(),
                            mode: self.mode.clone(),
                            kubernetes_version: self.kubernetes_version.clone(),
                            kube_conformance_image: self.kubernetes_conformance_image.clone(),
                        }
                        .into_map()
                        .context(error::ConfigMap)?,
                    ),
                },
            },
            status: None,
        };

        let tests = TestClient::new_from_k8s_client(k8s_client);

        tests.create_test(test).await.context(error::CreateTest)?;

        Ok(())
    }
}
