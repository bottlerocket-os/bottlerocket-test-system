use model::Configuration;
use resource_agent::clients::{AgentClient, ClientResult};
use resource_agent::provider::{ProviderError, Spec};
use resource_agent::{BootstrapData, ResourceAction};

/// Create an [`AgentClient`] that does nothing so that we can test without Kubernetes.
pub(crate) struct MockAgentClient;

#[async_trait::async_trait]
impl AgentClient for MockAgentClient {
    async fn new(_data: BootstrapData) -> ClientResult<Self> {
        Ok(Self {})
    }

    async fn send_init_error(&self, _action: ResourceAction, _error: &str) -> ClientResult<()> {
        Ok(())
    }

    async fn get_spec<Config>(&self) -> ClientResult<Spec<Config>>
    where
        Config: Configuration,
    {
        Ok(Spec::default())
    }

    async fn get_created_resource<Resource>(&self) -> ClientResult<Option<Resource>>
    where
        Resource: Configuration,
    {
        Ok(Some(Resource::default()))
    }

    /// Notify Kubernetes that the creation of resources is starting.
    async fn send_create_starting(&self) -> ClientResult<()> {
        Ok(())
    }

    async fn send_create_succeeded<Resource>(&self, _resource: Resource) -> ClientResult<()>
    where
        Resource: Configuration,
    {
        Ok(())
    }

    /// Notify Kubernetes that the creation of resources failed and provide an error message.
    async fn send_create_failed(&self, _error: &ProviderError) -> ClientResult<()> {
        Ok(())
    }

    /// Notify Kubernetes that the destruction of resources is starting.
    async fn send_destroy_starting(&self) -> ClientResult<()> {
        Ok(())
    }

    /// Notify Kubernetes that the destruction of resources succeeded.
    async fn send_destroy_succeeded(&self) -> ClientResult<()> {
        Ok(())
    }

    /// Notify Kubernetes that the destruction of resources failed and provide an error message.
    async fn send_destroy_failed(&self, _error: &ProviderError) -> ClientResult<()> {
        Ok(())
    }
}
