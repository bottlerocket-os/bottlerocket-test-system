use crate::error::{self, Result};
use bottlerocket_types::agent_config::{SonobuoyConfig, SonobuoyMode, AWS_CREDENTIALS_SECRET_NAME};
use kube::{api::ObjectMeta, Client};
use model::clients::CrdClient;
use model::constants::NAMESPACE;
use model::{clients::TestClient, Agent, Configuration, SecretName, Test, TestSpec};
use snafu::ResultExt;
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
    // TODO - default to an ECR public repository image
    #[structopt(long, short)]
    image: String,

    /// Name of the pull secret for the sonobuoy test image (if needed).
    #[structopt(long)]
    pull_secret: Option<String>,

    /// Keep the test running after completion.
    #[structopt(long)]
    keep_running: bool,

    /// The plugin used for the sonobuoy test. Normally this is `e2e` (the default).
    #[structopt(long, default_value = "e2e")]
    plugin: String,

    /// The test mode passed to the sonobuoy E2E plugin. We default to `quick` to make a quick test
    /// the most ergonomic.
    #[structopt(long, default_value = "quick")]
    mode: SonobuoyMode,

    /// The kubernetes conformance image used for the sonobuoy E2E plugin.
    #[structopt(long)]
    kubernetes_conformance_image: Option<String>,

    /// Path to config file that overrides the registries for test images.
    /// Specifying this option passes the config to `sonobuoy run --e2e-repo-config`
    #[structopt(long)]
    e2e_repo_config: Option<String>,

    /// The name of the secret containing aws credentials.
    #[structopt(long)]
    aws_secret: Option<SecretName>,

    /// The resources required by the sonobuoy test.
    #[structopt(long)]
    resource: Vec<String>,

    /// The arn for the role that should be assumed by the agent.
    #[structopt(long)]
    assume_role: Option<String>,
}

impl RunSonobuoy {
    pub(crate) async fn run(&self, k8s_client: Client) -> Result<()> {
        let kubeconfig_string = match (&self.target_cluster_kubeconfig_path, &self.target_cluster_kubeconfig) {
            (Some(kubeconfig_path), None) => base64::encode(
                read_to_string(kubeconfig_path).context(error::FileSnafu {
                    path: kubeconfig_path,
                })?,
            ),
            (None, Some(template_value)) => template_value.to_string(),
            (_, _) => return Err(error::Error::InvalidArguments { why: "Exactly 1 of 'target-cluster-kubeconfig' and 'target-cluster-kubeconfig-path' must be provided".to_string() })
        };

        let e2e_repo_config_string = match &self.e2e_repo_config {
            Some(e2e_repo_config_path) => Some(base64::encode(
                read_to_string(e2e_repo_config_path).context(error::FileSnafu {
                    path: e2e_repo_config_path,
                })?,
            )),
            None => None,
        };

        let test = Test {
            metadata: ObjectMeta {
                name: Some(self.name.clone()),
                namespace: Some(NAMESPACE.into()),
                ..Default::default()
            },
            spec: TestSpec {
                resources: self.resource.clone(),
                depends_on: Default::default(),
                retries: None,
                agent: Agent {
                    name: "sonobuoy-test-agent".to_string(),
                    image: self.image.clone(),
                    pull_secret: self.pull_secret.clone(),
                    keep_running: self.keep_running,
                    timeout: None,
                    configuration: Some(
                        SonobuoyConfig {
                            kubeconfig_base64: kubeconfig_string,
                            plugin: self.plugin.clone(),
                            mode: self.mode,
                            e2e_repo_config_base64: e2e_repo_config_string,
                            kubernetes_version: None,
                            kube_conformance_image: self.kubernetes_conformance_image.clone(),
                            assume_role: self.assume_role.clone(),
                        }
                        .into_map()
                        .context(error::ConfigMapSnafu)?,
                    ),
                    secrets: self.aws_secret.as_ref().map(|secret_name| {
                        let mut secrets_map = BTreeMap::new();
                        secrets_map
                            .insert(AWS_CREDENTIALS_SECRET_NAME.to_string(), secret_name.clone());
                        secrets_map
                    }),
                    // FIXME: Add CLI option for setting this
                    capabilities: None,
                },
            },
            status: None,
        };

        let tests = TestClient::new_from_k8s_client(k8s_client);

        tests.create(test).await.context(error::CreateTestSnafu)?;

        Ok(())
    }
}
