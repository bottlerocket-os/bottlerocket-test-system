use crate::{BootstrapData, Client, DefaultClient, TestInfo, TestResults};
use async_trait::async_trait;
use model::clients::{CrdClient, ResourceClient, TestClient};
use model::constants::TESTSYS_RESULTS_FILE;
use model::{Configuration, TaskState};
use serde_json::Value;
use snafu::{ResultExt, Snafu};
use std::fmt::{Debug, Display};
use std::path::PathBuf;
use tempfile::TempDir;

/// The public error type for the default [`Client`].
#[derive(Debug, Snafu)]
pub struct ClientError(InnerError);

/// The private error type for the default [`Client`].
#[derive(Debug, Snafu)]
pub(crate) enum InnerError {
    /// Any error when using the k8s client will have a descriptive error message. The user of
    /// `DefaultClient` is in a better position to provide context than we are, so we forward the
    /// error message.
    #[snafu(display("{}", source))]
    K8s { source: model::clients::Error },

    #[snafu(display("Unable to deserialize test configuration: {}", source))]
    Deserialization { source: serde_json::Error },

    #[snafu(display("Unable to create resource client: {}", source))]
    ResourceClientCreate { source: model::clients::Error },

    #[snafu(display("Unable to resolve config templates: {}", source))]
    ResolveConfig { source: model::clients::Error },

    #[snafu(display("An error occured while creating a `TempDir`: {}", source))]
    TempDirCreate { source: std::io::Error },
}

#[async_trait]
impl Client for DefaultClient {
    type E = ClientError;

    async fn new(bootstrap_data: BootstrapData) -> Result<Self, Self::E> {
        Ok(Self {
            client: TestClient::new().await.context(K8s)?,
            name: bootstrap_data.test_name,
            results_dir: TempDir::new().context(TempDirCreate)?,
        })
    }

    async fn keep_running(&self) -> Result<bool, Self::E> {
        let test_data = self.client.get(&self.name).await.context(K8s)?;
        Ok(test_data.spec.agent.keep_running)
    }

    async fn get_test_info<C>(&self) -> Result<TestInfo<C>, Self::E>
    where
        C: Configuration,
    {
        let test_data = self.client.get(&self.name).await.context(K8s)?;

        let raw_config = match test_data.spec.agent.configuration {
            Some(serde_map) => serde_map,
            None => Default::default(),
        };

        let resource_client = ResourceClient::new().await.context(ResourceClientCreate)?;
        let resolved_config = resource_client
            .resolve_templated_config(raw_config)
            .await
            .context(ResolveConfig)?;

        let configuration =
            serde_json::from_value(Value::Object(resolved_config)).context(Deserialization)?;

        Ok(TestInfo {
            name: self.name.clone(),
            configuration,
            secrets: test_data.spec.agent.secrets.unwrap_or_default(),
            results_dir: self.results_dir.path().to_path_buf(),
        })
    }

    async fn send_test_starting(&self) -> Result<(), Self::E> {
        self.client
            .send_agent_task_state(&self.name, TaskState::Running)
            .await
            .context(K8s)?;
        Ok(())
    }

    async fn send_test_done(&self, results: TestResults) -> Result<(), Self::E> {
        self.client
            .send_test_completed(&self.name, results)
            .await
            .context(K8s)?;
        Ok(())
    }

    async fn send_error<E>(&self, error: E) -> Result<(), Self::E>
    where
        E: Debug + Display + Send + Sync,
    {
        self.client
            .send_agent_error(&self.name, &error.to_string())
            .await
            .context(K8s)?;
        Ok(())
    }

    async fn results_directory(&self) -> Result<PathBuf, Self::E> {
        return Ok(self.results_dir.path().to_path_buf());
    }

    async fn results_file(&self) -> Result<PathBuf, Self::E> {
        return Ok(PathBuf::from(TESTSYS_RESULTS_FILE));
    }
}
