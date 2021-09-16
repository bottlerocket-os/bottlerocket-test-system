use super::error::ClientResult;
use crate::BootstrapData;
use client::model::Configuration;
use client::TestClient;

/// `InfoClient` allows [`Create`] and [`Destroy`] objects to store arbitrary information in the
/// Kubernetes status fields associated with the resource request. For example, you might want to
/// store a name or an ID of a resource even before the `create` process is done. That way, if a
/// failure occurs, you can retrieve that information in order to destroy the resources when
/// `destroy` is called.
///
/// You define a "plain old data" struct to represent the information that you want to store and
/// provide this type for the `Info` type parameter.
///
/// This is provided as a trait so that mock implementations can be injected into the [`Agent`] for
/// testing purposes. In practice you will use the [`DefaultInfoClient`].
///
#[async_trait::async_trait]
pub trait InfoClient: Sized + Send + Sync {
    /// Create a new `InfoClient` object.
    async fn new(data: BootstrapData) -> ClientResult<Self>;

    /// Get information from Kubernetes.
    async fn get_info<Info>(&self) -> ClientResult<Info>
    where
        Info: Configuration;

    /// Send (overwrite) information to Kubernetes.
    async fn send_info<Info>(&self, info: Info) -> ClientResult<()>
    where
        Info: Configuration;
}

/// Provides the default [`InfoClient`] implementation.
#[derive(Clone)]
pub struct DefaultInfoClient {
    pub(super) data: BootstrapData,
    pub(super) client: TestClient,
}
