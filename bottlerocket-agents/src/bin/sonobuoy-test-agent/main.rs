/*!

This is a test-agent for running sonobuoy Kubernetes tests.
It needs to run in a pod in a K8s cluster containing all the testsys-related CRDs.
(See yamlgen/deploy/testsys.yaml)

You can configure sonobuoy to run different types of plugins and tests.
See `SonobuoyConfig` for the different configuration values.

To build the container for the sonobuoy test agent, run `make sonobuoy-test-agent-image` from the
root directory of this repository.

Here is an example manifest for deploying the test definition for the sonobuoy test agent to a K8s cluster:

```yaml
apiVersion: testsys.bottlerocket.aws/v1
kind: Test
metadata:
  name: sonobuoy-e2e-full
  namespace: testsys-bottlerocket-aws
spec:
  agent:
    configuration:
      kubeconfig_base64: <Base64 encoded kubeconfig for the test cluster sonobuoy runs the tests in>
      plugin: e2e
      mode: certified-conformance
      kubernetes_version: v1.21.2
    image: <your sonobuoy-test-agent image URI>
    name: sonobuoy-test-agent
    keep_running: true
  resources: {}
```

!*/

use async_trait::async_trait;
use bottlerocket_agents::error::Error;
use bottlerocket_agents::sonobuoy::{delete_sonobuoy, run_sonobuoy};
use bottlerocket_agents::{
    decode_write_kubeconfig, init_agent_logger, setup_env, SonobuoyConfig,
    AWS_CREDENTIALS_SECRET_NAME, TEST_CLUSTER_KUBECONFIG_PATH,
};
use log::info;
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
        // Set up the aws credentials if they were provided.
        if let Some(aws_secret_name) = &self.aws_secret_name {
            setup_env(self, aws_secret_name).await?;
        }

        decode_write_kubeconfig(&self.config.kubeconfig_base64, TEST_CLUSTER_KUBECONFIG_PATH)
            .await?;
        run_sonobuoy(
            TEST_CLUSTER_KUBECONFIG_PATH,
            &self.config,
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
    init_agent_logger();
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
