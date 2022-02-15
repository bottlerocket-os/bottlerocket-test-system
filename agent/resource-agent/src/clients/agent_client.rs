use super::error::ClientResult;
use crate::provider::{ProviderError, Spec};
use crate::{BootstrapData, ResourceAction};
use model::clients::ResourceClient;
use model::Configuration;

/// `AgentClient` allows the [`Agent`] to communicate with Kubernetes.
///
/// This is provided as a trait so that mock implementations can be injected into the [`Agent`] for
/// testing purposes. In practice you will use the [`DefaultAgentClient`].
///
#[async_trait::async_trait]
pub trait AgentClient: Sized {
    /// Create a new `AgentClient`.
    async fn new(data: BootstrapData) -> ClientResult<Self>;

    /// If there is a problem during the `Agent::new` function, this will be used to send the error.
    async fn send_init_error(&self, action: ResourceAction, error: &str) -> ClientResult<()>;

    /// Get the resource specifications for this resource provider.
    async fn get_spec<Config>(&self) -> ClientResult<Spec<Config>>
    where
        Config: Configuration;

    /// Get the resource that this resource provider created. `None` if it hasn't been created.
    async fn get_created_resource<Resource>(&self) -> ClientResult<Option<Resource>>
    where
        Resource: Configuration;

    /// Notify Kubernetes that the creation of resources is starting.
    async fn send_create_starting(&self) -> ClientResult<()>;

    /// Notify Kubernetes that resource creation succeeded and provide the definition of the
    /// resource that was created.
    async fn send_create_succeeded<Resource>(&self, resource: Resource) -> ClientResult<()>
    where
        Resource: Configuration;

    /// Notify Kubernetes that the creation of resources failed and provide an error message.
    async fn send_create_failed(&self, error: &ProviderError) -> ClientResult<()>;

    /// Notify Kubernetes that the destruction of resources is starting.
    async fn send_destroy_starting(&self) -> ClientResult<()>;

    /// Notify Kubernetes that the destruction of resources succeeded.
    async fn send_destroy_succeeded(&self) -> ClientResult<()>;

    /// Notify Kubernetes that the destruction of resources failed and provide an error message.
    async fn send_destroy_failed(&self, error: &ProviderError) -> ClientResult<()>;

    async fn get_keep_running(&self) -> ClientResult<bool>;
}

/// Provides the default [`AgentClient`] implementation.
#[derive(Clone)]
pub struct DefaultAgentClient {
    pub(super) data: BootstrapData,
    pub(super) resource_client: ResourceClient,
}
