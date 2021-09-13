use super::error::{self, Result};
use crate::constants::NAMESPACE;
use crate::ResourceProvider;
use kube::Api;
use snafu::ResultExt;

/// An API Client for TestSys ResourceProvider CRD objects.
///
/// # Example
///
/// ```
///# use model::clients::ResourceProviderClient;
///# async fn no_run() {
/// let client = ResourceProviderClient::new().await.unwrap();
/// let rp = client.get_resource_provider("my-resource-provider").await.unwrap();
///# }
/// ```
#[derive(Clone)]
pub struct ResourceProviderClient {
    api: Api<ResourceProvider>,
}

impl ResourceProviderClient {
    /// Create a new [`ResourceProviderClient`] using either `KUBECONFIG` or the in-cluster
    /// environment variables.
    pub async fn new() -> Result<Self> {
        let k8s_client = kube::Client::try_default()
            .await
            .context(error::Initialization)?;
        Ok(Self {
            api: Api::<ResourceProvider>::namespaced(k8s_client, NAMESPACE),
        })
    }

    /// Get a [`ResourceProvider`] by name.
    pub async fn get_resource_provider<S>(&self, name: S) -> Result<ResourceProvider>
    where
        S: AsRef<str>,
    {
        Ok(self
            .api
            .get(name.as_ref())
            .await
            .context(error::KubeApiCall {
                method: "get",
                what: "resource provider",
            })?)
    }
}
