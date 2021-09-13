use async_trait::async_trait;
use model::clients::TestClient;
use model::model::{Configuration, RunState};
use serde_json::Value;
use snafu::{ResultExt, Snafu};
use std::fmt::{Debug, Display};

use crate::{BootstrapData, Client, DefaultClient, RunnerStatus, TestInfo};

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
}

#[async_trait]
impl Client for DefaultClient {
    type E = ClientError;

    async fn new(bootstrap_data: BootstrapData) -> Result<Self, Self::E> {
        Ok(Self {
            client: TestClient::new().await.context(K8s)?,
            name: bootstrap_data.test_name,
        })
    }

    async fn get_test_info<C>(&self) -> Result<TestInfo<C>, Self::E>
    where
        C: Configuration,
    {
        let test_data = self.client.get_test(&self.name).await.context(K8s)?;

        let configuration: C = match test_data.spec.agent.configuration {
            Some(serde_map) => {
                serde_json::from_value(Value::Object(serde_map)).context(Deserialization)?
            }
            None => Default::default(),
        };

        Ok(TestInfo {
            name: self.name.clone(),
            configuration,
        })
    }

    async fn send_status(&self, status: RunnerStatus) -> Result<(), Self::E> {
        let mut agent_status = self
            .client
            .get_agent_status(&self.name)
            .await
            .context(K8s)?;
        let (run_state, test_results) = match status {
            RunnerStatus::Running => (RunState::Running, None),
            RunnerStatus::Done(test_results) => (RunState::Done, Some(test_results)),
        };
        agent_status.run_state = run_state;
        agent_status.results = test_results;
        let _ = self
            .client
            .set_agent_status(&self.name, agent_status)
            .await
            .context(K8s)?;
        Ok(())
    }

    async fn is_cancelled(&self) -> Result<bool, Self::E> {
        let _test_data = self.client.get_test(&self.name).await.context(K8s)?;
        // TODO - check whether we are cancelled in a field of the test CRD
        Ok(false)
    }

    async fn send_error<E>(&self, error: E) -> Result<(), Self::E>
    where
        E: Debug + Display + Send + Sync,
    {
        let mut agent_status = self
            .client
            .get_agent_status(&self.name)
            .await
            .context(K8s)?;
        agent_status.run_state = RunState::Error;
        agent_status.error_message = Some(error.to_string());
        let _ = self
            .client
            .set_agent_status(&self.name, agent_status)
            .await
            .context(K8s)?;
        Ok(())
    }
}
