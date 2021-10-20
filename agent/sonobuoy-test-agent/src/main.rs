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
  resources: {}
```

!*/

use async_trait::async_trait;
use log::info;
use model::{Outcome, TestResults};
use simplelog::{Config as LogConfig, LevelFilter, SimpleLogger};
use snafu::{ensure, OptionExt, ResultExt, Snafu};
use sonobuoy_test_agent::SonobuoyConfig;
use std::path::Path;
use std::process::Command;
use std::{fs, process};
use test_agent::{BootstrapData, ClientError, DefaultClient, TestAgent, TestInfo};

const TEST_CLUSTER_KUBECONFIG: &str = "/local/test-cluster.kubeconfig";

#[derive(Debug, Snafu)]
enum SonobuoyError {
    #[snafu(display("Failed to base64-decode kubeconfig for test cluster: {}", source))]
    Base64Decode { source: base64::DecodeError },

    #[snafu(display("Failed to write kubeconfig for test cluster: {}", source))]
    KubeconfigWrite { source: std::io::Error },

    #[snafu(display("Failed to create sonobuoy process: {}", source))]
    SonobuoyProcess { source: std::io::Error },

    #[snafu(display("Failed to run conformance test"))]
    SonobuoyRun,

    #[snafu(display("Failed to clean-up sonobuoy resources"))]
    SonobuoyDelete,

    #[snafu(display("{}", source))]
    DeserializeJson { source: serde_json::Error },

    #[snafu(display("Missing '{}' field from sonobuoy status", field))]
    MissingSonobuoyStatusField { field: String },
}

struct SonobuoyTestRunner {
    config: SonobuoyConfig,
}

#[async_trait]
impl test_agent::Runner for SonobuoyTestRunner {
    type C = SonobuoyConfig;
    type E = SonobuoyError;

    async fn new(test_info: TestInfo<Self::C>) -> Result<Self, Self::E> {
        info!("Initializing Sonobuoy test agent...");
        Ok(Self {
            config: test_info.configuration,
        })
    }

    async fn run(&mut self) -> Result<TestResults, Self::E> {
        info!("Decoding kubeconfig for test cluster");
        let decoded_bytes =
            base64::decode(self.config.kubeconfig_base64.as_bytes()).context(Base64Decode)?;
        let path = Path::new(TEST_CLUSTER_KUBECONFIG);
        info!("Storing kubeconfig in {}", path.display());
        fs::write(path, decoded_bytes).context(KubeconfigWrite)?;
        let kubconfig_arg = vec!["--kubeconfig", TEST_CLUSTER_KUBECONFIG];
        let k8s_image_arg = match (
            &self.config.kube_conformance_image,
            &self.config.kubernetes_version,
        ) {
            (Some(image), None) | (Some(image), Some(_)) => {
                vec!["--kube-conformance-image", image]
            }
            (None, Some(version)) => {
                vec!["--kubernetes-version", version]
            }
            _ => {
                vec![]
            }
        };
        info!("Running sonobuoy");
        let status = Command::new("/usr/bin/sonobuoy")
            .args(kubconfig_arg.to_owned())
            .arg("run")
            .arg("--wait")
            .arg("--plugin")
            .arg(&self.config.plugin)
            .arg("--mode")
            .arg(&self.config.mode)
            .args(k8s_image_arg)
            .status()
            .context(SonobuoyProcess)?;
        ensure!(status.success(), SonobuoyRun);

        let run_result = Command::new("/usr/bin/sonobuoy")
            .args(kubconfig_arg)
            .arg("status")
            .arg("--json")
            .output()
            .context(SonobuoyProcess)?;

        let run_status: serde_json::Value =
            serde_json::from_str(&String::from_utf8_lossy(&run_result.stdout))
                .context(DeserializeJson)?;
        let e2e_status = run_status
            .get("plugins")
            .context(MissingSonobuoyStatusField { field: "plugins" })?
            .as_array()
            .context(MissingSonobuoyStatusField { field: "plugins" })?
            .first()
            .context(MissingSonobuoyStatusField {
                field: format!("plugins.{}", self.config.plugin),
            })?;
        let progress_status = e2e_status
            .get("progress")
            .context(MissingSonobuoyStatusField {
                field: format!("plugins.{}.progress", self.config.plugin),
            })?;
        let result_status = e2e_status
            .get("result-status")
            .context(MissingSonobuoyStatusField {
                field: format!("plugins.{}.result-status", self.config.plugin),
            })?
            .as_str()
            .context(MissingSonobuoyStatusField {
                field: format!("plugins.{}.result-status", self.config.plugin),
            })?;
        let result_counts = run_status
            .get("plugins")
            .context(MissingSonobuoyStatusField { field: "plugins" })?
            .as_array()
            .context(MissingSonobuoyStatusField { field: "plugins" })?
            .first()
            .context(MissingSonobuoyStatusField {
                field: format!("plugins.{}", self.config.plugin),
            })?
            .get("result-counts")
            .context(MissingSonobuoyStatusField {
                field: format!("plugins.{}.result-counts", self.config.plugin),
            })?;
        let num_passed = result_counts
            .get("passed")
            .map(|v| v.as_u64().unwrap_or(0))
            .unwrap_or(0);
        let num_failed = result_counts
            .get("failed")
            .map(|v| v.as_u64().unwrap_or(0))
            .unwrap_or(0);
        let num_skipped = result_counts
            .get("skipped")
            .map(|v| v.as_u64().unwrap_or(0))
            .unwrap_or(0);

        Ok(TestResults {
            outcome: match result_status {
                "pass" | "passed" => Outcome::Pass,
                "fail" | "failed" => Outcome::Fail,
                "timeout" | "timed-out" => Outcome::Timeout,
                _ => Outcome::Unknown,
            },
            num_passed,
            num_failed,
            num_skipped,
            other_info: Some(progress_status.to_owned().to_string()),
        })
    }

    async fn terminate(&mut self) -> Result<(), Self::E> {
        let kubconfig_arg = vec!["--kubeconfig", TEST_CLUSTER_KUBECONFIG];

        info!("Deleting sonobuoy resources from cluster");
        let status = Command::new("/usr/bin/sonobuoy")
            .args(kubconfig_arg)
            .arg("delete")
            .arg("--wait")
            .status()
            .context(SonobuoyProcess)?;
        ensure!(status.success(), SonobuoyDelete);

        Ok(())
    }
}

#[tokio::main]
async fn main() {
    // SimpleLogger will send errors to stderr and anything less to stdout.
    if let Err(e) = SimpleLogger::init(LevelFilter::Info, LogConfig::default()) {
        eprintln!("{}", e);
        process::exit(1);
    }
    if let Err(e) = run().await {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}
async fn run() -> Result<(), test_agent::error::Error<ClientError, SonobuoyError>> {
    let mut agent = TestAgent::<DefaultClient, SonobuoyTestRunner>::new(
        BootstrapData::from_env().unwrap_or_else(|_| BootstrapData {
            test_name: "sonobuoy_test".to_string(),
        }),
    )
    .await?;
    agent.run().await
}
