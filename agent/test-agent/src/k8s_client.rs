use crate::error::{InfoClientError, InfoClientResult};
use crate::{
    BootstrapData, Client, DefaultClient, DefaultInfoClient, InfoClient, Spec, TestResults,
};
use async_trait::async_trait;
use serde_json::Value;
use snafu::{ResultExt, Snafu};
use std::fmt::{Debug, Display};
use std::path::PathBuf;
use tempfile::TempDir;
use testsys_model::clients::{CrdClient, ResourceClient, TestClient};
use testsys_model::constants::TESTSYS_RESULTS_FILE;
use testsys_model::{Configuration, TaskState};

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
    K8s {
        source: testsys_model::clients::Error,
    },

    #[snafu(display("Unable to deserialize test configuration: {}", source))]
    Deserialization { source: serde_json::Error },

    #[snafu(display("Unable to create resource client: {}", source))]
    ResourceClientCreate {
        source: testsys_model::clients::Error,
    },

    #[snafu(display("Unable to resolve config templates: {}", source))]
    ResolveConfig {
        source: testsys_model::clients::Error,
    },

    #[snafu(display("An error occurred while creating a `TempDir`: {}", source))]
    TempDirCreate { source: std::io::Error },
}

#[async_trait]
impl Client for DefaultClient {
    type E = ClientError;

    async fn new(bootstrap_data: BootstrapData) -> Result<Self, Self::E> {
        Ok(Self {
            client: TestClient::new().await.context(K8sSnafu)?,
            name: bootstrap_data.test_name,
            results_dir: TempDir::new().context(TempDirCreateSnafu)?,
        })
    }

    async fn keep_running(&self) -> Result<bool, Self::E> {
        let test_data = self.client.get(&self.name).await.context(K8sSnafu)?;
        Ok(test_data.spec.agent.keep_running)
    }

    async fn retries(&self) -> Result<u32, Self::E> {
        let test_data = self.client.get(&self.name).await.context(K8sSnafu)?;
        Ok(test_data.spec.retries.unwrap_or_default())
    }

    async fn spec<C>(&self) -> Result<Spec<C>, Self::E>
    where
        C: Configuration,
    {
        let test_data = self.client.get(&self.name).await.context(K8sSnafu)?;

        let raw_config = match test_data.spec.agent.configuration {
            Some(serde_map) => serde_map,
            None => Default::default(),
        };

        let resource_client = ResourceClient::new()
            .await
            .context(ResourceClientCreateSnafu)?;
        let resolved_config = resource_client
            .resolve_templated_config(raw_config)
            .await
            .context(ResolveConfigSnafu)?;

        let configuration =
            serde_json::from_value(Value::Object(resolved_config)).context(DeserializationSnafu)?;

        Ok(Spec {
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
            .context(K8sSnafu)?;
        Ok(())
    }

    async fn send_test_completed(&self) -> Result<(), Self::E> {
        self.client
            .send_agent_task_state(&self.name, TaskState::Completed)
            .await
            .context(K8sSnafu)?;
        Ok(())
    }

    async fn send_test_update(&self, results: TestResults) -> Result<(), Self::E> {
        self.client
            .send_test_update(&self.name, results)
            .await
            .context(K8sSnafu)?;
        Ok(())
    }

    async fn send_test_results(&self, results: TestResults) -> Result<(), Self::E> {
        self.client
            .send_test_results(&self.name, results)
            .await
            .context(K8sSnafu)?;
        Ok(())
    }

    async fn send_error<E>(&self, error: E) -> Result<(), Self::E>
    where
        E: Debug + Display + Send + Sync,
    {
        self.client
            .send_agent_error(&self.name, &error.to_string())
            .await
            .context(K8sSnafu)?;
        Ok(())
    }

    async fn results_directory(&self) -> Result<PathBuf, Self::E> {
        return Ok(self.results_dir.path().to_path_buf());
    }

    async fn results_file(&self) -> Result<PathBuf, Self::E> {
        return Ok(PathBuf::from(TESTSYS_RESULTS_FILE));
    }
}

#[async_trait::async_trait]
impl InfoClient for DefaultInfoClient {
    async fn new(d: BootstrapData) -> InfoClientResult<Self> {
        Ok(Self {
            client: TestClient::new()
                .await
                .map_err(|e| InfoClientError::InitializationFailed(Some(e.into())))?,
            data: d,
        })
    }

    async fn send_test_update(&self, results: TestResults) -> InfoClientResult<()> {
        self.client
            .send_test_update(&self.data.test_name, results)
            .await
            .map_err(|e| InfoClientError::RequestFailed(Some(e.into())))?;
        Ok(())
    }
}
