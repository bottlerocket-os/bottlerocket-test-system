use super::error::ClientResult;
use crate::clients::{AgentClient, ClientError, DefaultAgentClient, DefaultInfoClient, InfoClient};
use crate::provider::{ProviderError, Resources, Spec};
use crate::{BootstrapData, ResourceAction};
use agent_common::secrets::{SecretData, SecretsReader};
use model::clients::{CrdClient, ResourceClient};
use model::{
    Configuration, Error as ModelError, ErrorResources, ResourceError, SecretName, TaskState,
};

impl From<model::clients::Error> for ClientError {
    fn from(e: model::clients::Error) -> Self {
        ClientError::RequestFailed(Some(Box::new(e)))
    }
}

impl From<ModelError> for ClientError {
    fn from(e: ModelError) -> Self {
        ClientError::Serialization(Some(Box::new(e)))
    }
}

impl From<Resources> for ErrorResources {
    fn from(r: Resources) -> Self {
        match r {
            Resources::Orphaned => ErrorResources::Orphaned,
            Resources::Remaining => ErrorResources::Remaining,
            Resources::Clear => ErrorResources::Clear,
            Resources::Unknown => ErrorResources::Unknown,
        }
    }
}

#[async_trait::async_trait]
impl InfoClient for DefaultInfoClient {
    async fn new(data: BootstrapData) -> ClientResult<Self> {
        let client = ResourceClient::new()
            .await
            .map_err(|e| ClientError::InitializationFailed(Some(Box::new(e))))?;
        Ok(Self { data, client })
    }

    async fn get_info<Info>(&self) -> ClientResult<Info>
    where
        Info: Configuration,
    {
        Ok(self.client.get_agent_info(&self.data.resource_name).await?)
    }

    async fn send_info<Info>(&self, info: Info) -> ClientResult<()>
    where
        Info: Configuration,
    {
        let _ = self
            .client
            .send_agent_info(&self.data.resource_name, info)
            .await?;
        Ok(())
    }

    async fn get_secret(&self, secret_name: &SecretName) -> ClientResult<SecretData> {
        let secret_reader = SecretsReader::new();
        secret_reader
            .get_secret(secret_name)
            .map_err(|e| ClientError::SecretsError(Some(Box::new(e))))
    }
}

#[async_trait::async_trait]
impl AgentClient for DefaultAgentClient {
    async fn new(data: BootstrapData) -> ClientResult<Self> {
        Ok(Self {
            data,
            resource_client: ResourceClient::new()
                .await
                .map_err(|e| ClientError::InitializationFailed(Some(Box::new(e))))?,
        })
    }

    async fn send_init_error(&self, action: ResourceAction, error: &str) -> ClientResult<()> {
        let e = ResourceError {
            error: error.into(),
            error_resources: ErrorResources::Unknown,
        };
        self.resource_client
            .send_error(&self.data.resource_name, action, &e)
            .await?;
        Ok(())
    }

    async fn get_spec<Config>(&self) -> ClientResult<Spec<Config>>
    where
        Config: Configuration,
    {
        let resource = self.resource_client.get(&self.data.resource_name).await?;
        let config = Config::from_map(
            self.resource_client
                .resolve_templated_config(resource.spec.agent.configuration.unwrap_or_default())
                .await?,
        )?;
        Ok(Spec {
            configuration: config,
            secrets: resource.spec.agent.secrets.unwrap_or_default(),
        })
    }

    async fn get_created_resource<Resource>(&self) -> ClientResult<Option<Resource>>
    where
        Resource: Configuration,
    {
        Ok(self
            .resource_client
            .get_created_resource(&self.data.resource_name)
            .await?)
    }

    async fn send_create_starting(&self) -> ClientResult<()> {
        let _ = self
            .resource_client
            .send_task_state(
                &self.data.resource_name,
                ResourceAction::Create,
                TaskState::Running,
            )
            .await?;
        Ok(())
    }

    async fn send_create_succeeded<Resource>(&self, resource: Resource) -> ClientResult<()>
    where
        Resource: Configuration,
    {
        let _ = self
            .resource_client
            .send_creation_success(&self.data.resource_name, resource)
            .await?;
        Ok(())
    }

    async fn send_create_failed(&self, error: &ProviderError) -> ClientResult<()> {
        let _ = self
            .resource_client
            .send_error(
                &self.data.resource_name,
                ResourceAction::Create,
                &ResourceError {
                    error: format!("{}", error),
                    error_resources: error.resources().into(),
                },
            )
            .await?;
        Ok(())
    }

    async fn send_destroy_starting(&self) -> ClientResult<()> {
        let _ = self
            .resource_client
            .send_task_state(
                &self.data.resource_name,
                ResourceAction::Destroy,
                TaskState::Running,
            )
            .await?;
        Ok(())
    }

    async fn send_destroy_succeeded(&self) -> ClientResult<()> {
        let _ = self
            .resource_client
            .send_task_state(
                &self.data.resource_name,
                ResourceAction::Destroy,
                TaskState::Completed,
            )
            .await?;
        Ok(())
    }

    async fn send_destroy_failed(&self, error: &ProviderError) -> ClientResult<()> {
        let _ = self
            .resource_client
            .send_error(
                &self.data.resource_name,
                ResourceAction::Destroy,
                &ResourceError {
                    error: format!("{}", error),
                    error_resources: error.resources().into(),
                },
            )
            .await?;
        Ok(())
    }
}
