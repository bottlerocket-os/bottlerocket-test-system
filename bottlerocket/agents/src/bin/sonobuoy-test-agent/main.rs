/*!

This is a test-agent for running sonobuoy Kubernetes tests.
It expects to be run in a pod launched by the TestSys controller.

You can configure sonobuoy to run different types of plugins and tests.
See `SonobuoyConfig` for the different configuration values.

To build the container for the sonobuoy test agent, run `make sonobuoy-test-agent-image` from the
root directory of this repository.

Here is an example manifest for deploying the test definition for the sonobuoy test agent to a K8s cluster:

```yaml
apiVersion: testsys.system/v1
kind: Test
metadata:
  name: sonobuoy-e2e-full
  namespace: testsys
spec:
  agent:
    configuration:
      kubeconfigBase64: <Base64 encoded kubeconfig for the test cluster sonobuoy runs the tests in>
      plugin: e2e
      mode: certified-conformance
      kubernetes_version: v1.21.2
    image: <your sonobuoy-test-agent image URI>
    name: sonobuoy-test-agent
    keepRunning: true
  resources: {}
```

!*/

use agent_utils::aws::aws_config;
use agent_utils::{base64_decode_write_file, init_agent_logger};
use async_trait::async_trait;
use bottlerocket_agents::constants::{E2E_REPO_CONFIG_PATH, TEST_CLUSTER_KUBECONFIG_PATH};
use bottlerocket_agents::error::Error;
use bottlerocket_agents::sonobuoy::{delete_sonobuoy, rerun_failed_sonobuoy, run_sonobuoy};
use bottlerocket_types::agent_config::{SonobuoyConfig, AWS_CREDENTIALS_SECRET_NAME};
use log::{debug, info};
use model::{SecretName, TestResults};
use std::path::PathBuf;
use test_agent::{BootstrapData, ClientError, DefaultClient, Spec, TestAgent};

struct SonobuoyTestRunner {
    config: SonobuoyConfig,
    aws_secret_name: Option<SecretName>,
    results_dir: PathBuf,
}

#[async_trait]
impl test_agent::Runner for SonobuoyTestRunner {
    type C = SonobuoyConfig;
    type E = Error;

    async fn new(spec: Spec<Self::C>) -> Result<Self, Self::E> {
        info!("Initializing Sonobuoy test agent...");
        Ok(Self {
            config: spec.configuration,
            aws_secret_name: spec.secrets.get(AWS_CREDENTIALS_SECRET_NAME).cloned(),
            results_dir: spec.results_dir,
        })
    }

    async fn run(&mut self) -> Result<TestResults, Self::E> {
        aws_config(
            &self.aws_secret_name.as_ref(),
            &self.config.assume_role,
            &None,
            &None,
            true,
        )
        .await?;

        debug!("Decoding kubeconfig for test cluster");
        base64_decode_write_file(&self.config.kubeconfig_base64, TEST_CLUSTER_KUBECONFIG_PATH)
            .await?;
        info!("Stored kubeconfig in {}", TEST_CLUSTER_KUBECONFIG_PATH);
        let e2e_repo_config = match &self.config.e2e_repo_config_base64 {
            Some(e2e_repo_config_base64) => {
                info!("Decoding e2e-repo-config config");
                base64_decode_write_file(e2e_repo_config_base64, E2E_REPO_CONFIG_PATH).await?;
                info!("Stored e2e-repo-config in {}", E2E_REPO_CONFIG_PATH);
                Some(E2E_REPO_CONFIG_PATH)
            }
            None => None,
        };

        run_sonobuoy(
            TEST_CLUSTER_KUBECONFIG_PATH,
            e2e_repo_config,
            &self.config,
            &self.results_dir,
        )
        .await
    }

    async fn rerun_failed(&mut self, _prev_results: &TestResults) -> Result<TestResults, Self::E> {
        // Set up the aws credentials if they were provided.
        aws_config(
            &self.aws_secret_name.as_ref(),
            &self.config.assume_role,
            &None,
            &None,
            true,
        )
        .await?;

        delete_sonobuoy(TEST_CLUSTER_KUBECONFIG_PATH).await?;

        debug!("Decoding kubeconfig for test cluster");
        base64_decode_write_file(&self.config.kubeconfig_base64, TEST_CLUSTER_KUBECONFIG_PATH)
            .await?;
        info!("Stored kubeconfig in {}", TEST_CLUSTER_KUBECONFIG_PATH);
        let e2e_repo_config = match &self.config.e2e_repo_config_base64 {
            Some(e2e_repo_config_base64) => {
                info!("Decoding e2e-repo-config config");
                base64_decode_write_file(e2e_repo_config_base64, E2E_REPO_CONFIG_PATH).await?;
                info!("Stored e2e-repo-config in {}", E2E_REPO_CONFIG_PATH);
                Some(E2E_REPO_CONFIG_PATH)
            }
            None => None,
        };

        rerun_failed_sonobuoy(
            TEST_CLUSTER_KUBECONFIG_PATH,
            e2e_repo_config,
            &self.results_dir,
        )
        .await
    }

    async fn terminate(&mut self) -> Result<(), Self::E> {
        delete_sonobuoy(TEST_CLUSTER_KUBECONFIG_PATH).await
    }
}

#[tokio::main]
async fn main() {
    init_agent_logger(env!("CARGO_CRATE_NAME"), None);
    if let Err(e) = run().await {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}

async fn run() -> Result<(), test_agent::error::Error<ClientError, Error>> {
    let mut agent = TestAgent::<DefaultClient, SonobuoyTestRunner>::new(
        BootstrapData::from_env().unwrap_or_else(|_| BootstrapData {
            test_name: "sonobuoy_test".to_string(),
        }),
    )
    .await?;
    agent.run().await
}
