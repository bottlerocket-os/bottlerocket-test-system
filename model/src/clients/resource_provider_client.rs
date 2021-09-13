use crate::model::{ResourceProvider, NAMESPACE};
use kube::Api;
use snafu::{ResultExt, Snafu};

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

/// The `Result` type returned by [`ResourceProviderClient`].
pub type Result<T> = std::result::Result<T, Error>;

/// The public error type for `ResourceProvider`.
// TODO - consolidate error types https://github.com/bottlerocket-os/bottlerocket-test-system/issues/91
#[derive(Debug, Snafu)]
pub struct Error(InnerError);

/// The private error type for `ResourceProvider`.
#[derive(Debug, Snafu)]
pub(crate) enum InnerError {
    #[snafu(display("Error initializing the Kubernetes client: {}", source))]
    Initialization { source: kube::Error },

    #[snafu(display("Unable to {} {}: {}", method, what, source))]
    KubeApiCall {
        method: String,
        what: String,
        source: kube::Error,
    },
}

impl ResourceProviderClient {
    /// Create a new [`ResourceProviderClient`] using either `KUBECONFIG` or the in-cluster
    /// environment variables.
    pub async fn new() -> Result<Self> {
        let k8s_client = kube::Client::try_default().await.context(Initialization)?;
        Ok(Self {
            api: Api::<ResourceProvider>::namespaced(k8s_client, NAMESPACE),
        })
    }

    /// Get a [`ResourceProvider`] by name.
    pub async fn get_resource_provider<S>(&self, name: S) -> Result<ResourceProvider>
    where
        S: AsRef<str>,
    {
        Ok(self.api.get(name.as_ref()).await.context(KubeApiCall {
            method: "get",
            what: "resource provider",
        })?)
    }
}
