use crate::error::{self, K8sSnafu, ResolveConfigSnafu, ResourceClientCreateSnafu, Result};
use model::clients::{CrdClient, ResourceClient, TestClient};
use model::constants::TESTSYS_RESULTS_FILE;
use model::{SecretName, SecretType, TaskState, TestResults};
use serde_json::{Map, Value};
use snafu::ResultExt;
use std::collections::BTreeMap;
use std::path::PathBuf;
use tempfile::TempDir;

// This client struct can help to send and receive the information from/to Kubernetes cluster
pub struct Client {
    client: TestClient,
    name: String,
    results_dir: TempDir,
}

#[derive(Debug, Clone)]
pub struct Spec {
    pub name: String,
    pub configuration: Map<String, Value>,
    pub secrets: BTreeMap<SecretType, SecretName>,
    pub results_dir: PathBuf,
}

impl Client {
    // Create new Kubernetes client using the test_name
    pub(crate) async fn new(test_name: String) -> Result<Self> {
        Ok(Self {
            client: TestClient::new().await.context(K8sSnafu)?,
            name: test_name,
            results_dir: TempDir::new().context(error::TempDirCreateSnafu)?,
        })
    }

    // Get the test specializations like configuration secrets
    pub(crate) async fn spec(&self) -> Result<Spec> {
        let test_data = self.client.get(&self.name).await.context(K8sSnafu)?;

        let raw_config = match test_data.spec.agent.configuration {
            Some(serde_map) => serde_map,
            None => Default::default(),
        };

        let resource_client = ResourceClient::new()
            .await
            .context(ResourceClientCreateSnafu)?;

        let configuration = resource_client
            .resolve_templated_config(raw_config)
            .await
            .context(ResolveConfigSnafu)?;

        Ok(Spec {
            name: self.name.clone(),
            configuration,
            secrets: test_data.spec.agent.secrets.unwrap_or_default(),
            results_dir: self.results_dir.path().to_path_buf(),
        })
    }

    // Get no of retries to rerun the test
    pub(crate) async fn retries(&self) -> Result<u32> {
        let test_data = self.client.get(&self.name).await.context(K8sSnafu)?;
        Ok(test_data.spec.retries.unwrap_or_default())
    }

    // Change the task_status running
    pub(crate) async fn send_test_starting(&self) -> Result<()> {
        self.client
            .send_agent_task_state(&self.name, TaskState::Running)
            .await
            .context(K8sSnafu)?;
        Ok(())
    }

    // Check the keep running flag for the pod
    pub(crate) async fn keep_running(&self) -> Result<bool> {
        let test_data = self.client.get(&self.name).await.context(K8sSnafu)?;
        Ok(test_data.spec.agent.keep_running)
    }

    // Change the task_status as completed
    pub(crate) async fn send_test_completed(&self) -> Result<()> {
        self.client
            .send_agent_task_state(&self.name, TaskState::Completed)
            .await
            .context(K8sSnafu)?;
        Ok(())
    }

    // Get the path for results directory
    pub(crate) async fn results_directory(&self) -> Result<PathBuf> {
        return Ok(self.results_dir.path().to_path_buf());
    }

    // Get path for results file
    pub(crate) async fn results_file(&self) -> Result<PathBuf> {
        Ok(PathBuf::from(TESTSYS_RESULTS_FILE))
    }

    // Change task_status as error and send the error occurred while performing test
    pub(crate) async fn send_error(&self, error: &str) -> Result<()> {
        self.client
            .send_agent_error(&self.name, error)
            .await
            .context(K8sSnafu)?;
        Ok(())
    }

    // Send test results to kubernetes cluster
    pub(crate) async fn send_test_results(&self, results: TestResults) -> Result<()> {
        self.client
            .send_test_results(&self.name, results)
            .await
            .context(K8sSnafu)?;
        Ok(())
    }
}
