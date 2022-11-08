/*!

This is a test-agent for running workload tests on Kubernetes.
It expects to be run in a pod launched by the TestSys controller.

You can configure the workload agent to run different types of plugins and tests.
See `WorkloadConfig` for the different configuration values.

To build the container for the workload test agent, run `make k8s-workload-agent-image` from the
root directory of this repository.

Here is an example manifest for deploying the test definition for the workload test agent to a K8s cluster:

```yaml
apiVersion: testsys.system/v1
kind: Test
metadata:
  name: workload-full
  namespace: testsys
spec:
  agent:
    configuration:
      kubeconfigBase64: <Base64 encoded kubeconfig for the test cluster workload runs the tests in>
      plugins:
      - name: nvidia-workload
        image: testsys-nvidia-workload-test:v0.0.3
    image: <your k8s-workload-agent image URI>
    name: workload-test-agent
    keepRunning: true
  resources: {}
```

!*/

use agent_utils::{base64_decode_write_file, init_agent_logger};
use async_trait::async_trait;
use bottlerocket_agents::constants::TEST_CLUSTER_KUBECONFIG_PATH;
use bottlerocket_agents::error::Error;
use bottlerocket_agents::workload::{delete_workload, rerun_failed_workload, run_workload};
use bottlerocket_types::agent_config::WorkloadConfig;
use log::info;
use model::TestResults;
use std::path::PathBuf;
use test_agent::{BootstrapData, ClientError, DefaultClient, Spec, TestAgent};

struct WorkloadTestRunner {
    config: WorkloadConfig,
    results_dir: PathBuf,
}

#[async_trait]
impl test_agent::Runner for WorkloadTestRunner {
    type C = WorkloadConfig;
    type E = Error;

    async fn new(spec: Spec<Self::C>) -> Result<Self, Self::E> {
        info!("Initializing Workload test agent...");
        Ok(Self {
            config: spec.configuration,
            results_dir: spec.results_dir,
        })
    }

    async fn run(&mut self) -> Result<TestResults, Self::E> {
        info!("Decoding kubeconfig for test cluster");
        base64_decode_write_file(&self.config.kubeconfig_base64, TEST_CLUSTER_KUBECONFIG_PATH)
            .await?;
        info!("Stored kubeconfig in {}", TEST_CLUSTER_KUBECONFIG_PATH);

        run_workload(
            TEST_CLUSTER_KUBECONFIG_PATH,
            &self.config,
            &self.results_dir,
        )
        .await
    }

    async fn rerun_failed(&mut self, _prev_results: &TestResults) -> Result<TestResults, Self::E> {
        delete_workload(TEST_CLUSTER_KUBECONFIG_PATH).await?;

        info!("Decoding kubeconfig for test cluster");
        base64_decode_write_file(&self.config.kubeconfig_base64, TEST_CLUSTER_KUBECONFIG_PATH)
            .await?;
        info!("Stored kubeconfig in {}", TEST_CLUSTER_KUBECONFIG_PATH);

        rerun_failed_workload(TEST_CLUSTER_KUBECONFIG_PATH, &self.results_dir).await
    }

    async fn terminate(&mut self) -> Result<(), Self::E> {
        delete_workload(TEST_CLUSTER_KUBECONFIG_PATH).await
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
    let mut agent = TestAgent::<DefaultClient, WorkloadTestRunner>::new(
        BootstrapData::from_env().unwrap_or_else(|_| BootstrapData {
            test_name: "workload_test".to_string(),
        }),
    )
    .await?;
    agent.run().await
}
