/*!

This is a test-agent for running workload tests on Kubernetes.
It expects to be run in a pod launched by the TestSys controller.

You can configure the workload agent to run different types of plugins and tests.
See `WorkloadConfig` for the different configuration values.

To build the container for the workload test agent, run `make k8s-workload-agent` from the
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
      tests:
      - name: nvidia-workload
        image: testsys-nvidia-workload-test:v0.0.3
        gpu: false
    image: <your k8s-workload-agent image URI>
    name: workload-test-agent
    keepRunning: true
  resources: {}
```

!*/

use agent_utils::aws::aws_config;
use agent_utils::{base64_decode_write_file, init_agent_logger};
use async_trait::async_trait;
use bottlerocket_agents::constants::TEST_CLUSTER_KUBECONFIG_PATH;
use bottlerocket_agents::error::Error;
use bottlerocket_agents::workload::{delete_workload, rerun_failed_workload, run_workload};
use bottlerocket_types::agent_config::{WorkloadConfig, AWS_CREDENTIALS_SECRET_NAME};
use log::info;
use model::{SecretName, TestResults};
use std::path::PathBuf;
use test_agent::{
    BootstrapData, ClientError, DefaultClient, DefaultInfoClient, InfoClient, Spec, TestAgent,
};

struct WorkloadTestRunner {
    config: WorkloadConfig,
    aws_secret_name: Option<SecretName>,
    results_dir: PathBuf,
}

#[async_trait]
impl<I> test_agent::Runner<I> for WorkloadTestRunner
where
    I: InfoClient,
{
    type C = WorkloadConfig;
    type E = Error;

    async fn new(spec: Spec<Self::C>, _info_client: &I) -> Result<Self, Self::E> {
        info!("Initializing Workload test agent...");
        Ok(Self {
            config: spec.configuration,
            aws_secret_name: spec.secrets.get(AWS_CREDENTIALS_SECRET_NAME).cloned(),
            results_dir: spec.results_dir,
        })
    }

    async fn run(&mut self, info_client: &I) -> Result<TestResults, Self::E> {
        aws_config(
            &self.aws_secret_name.as_ref(),
            &self.config.assume_role,
            &None,
            &None,
            true,
        )
        .await?;

        info!("Decoding kubeconfig for test cluster");
        base64_decode_write_file(&self.config.kubeconfig_base64, TEST_CLUSTER_KUBECONFIG_PATH)
            .await?;
        info!("Stored kubeconfig in {}", TEST_CLUSTER_KUBECONFIG_PATH);

        run_workload(
            TEST_CLUSTER_KUBECONFIG_PATH,
            &self.config,
            &self.results_dir,
            info_client,
        )
        .await
    }

    async fn rerun_failed(
        &mut self,
        _prev_results: &TestResults,
        info_client: &I,
    ) -> Result<TestResults, Self::E> {
        delete_workload(TEST_CLUSTER_KUBECONFIG_PATH).await?;

        info!("Decoding kubeconfig for test cluster");
        base64_decode_write_file(&self.config.kubeconfig_base64, TEST_CLUSTER_KUBECONFIG_PATH)
            .await?;
        info!("Stored kubeconfig in {}", TEST_CLUSTER_KUBECONFIG_PATH);

        rerun_failed_workload(TEST_CLUSTER_KUBECONFIG_PATH, &self.results_dir, info_client).await
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
    let mut agent = TestAgent::<DefaultClient, WorkloadTestRunner, DefaultInfoClient>::new(
        BootstrapData::from_env().unwrap_or_else(|_| BootstrapData {
            test_name: "workload_test".to_string(),
        }),
    )
    .await?;
    agent.run().await
}
