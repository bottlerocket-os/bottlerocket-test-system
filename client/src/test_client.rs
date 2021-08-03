use crate::model::{AgentStatus, Test, TESTSYS_API, TESTSYS_NAMESPACE};
use kube::api::{Patch, PatchParams};
use kube::Api;
use serde_json::json;
use snafu::{ResultExt, Snafu};

/// An API Client for TestSys Test CRD objects.
///
/// # Example
///
/// ```
///# use client::TestClient;
///# async fn no_run() {
/// let test_client = TestClient::new().await.unwrap();
/// let test = test_client.get_test("my-test").await.unwrap();
///# }
/// ```
#[derive(Clone)]
pub struct TestClient {
    api: Api<Test>,
}

/// The `Result` type returned by [`TestClient`].
pub type Result<T> = std::result::Result<T, Error>;

/// The public error type for `TestClient`.
#[derive(Debug, Snafu)]
pub struct Error(InnerError);

/// The private error type for `TestClient`.
#[derive(Debug, Snafu)]
pub(crate) enum InnerError {
    #[snafu(display("Error initializing the Kubernetes client: {}", source))]
    Initialization { source: kube::Error },

    #[snafu(display("Unable to {} {}: {}", method, resource, source))]
    KubeApiCall {
        method: String,
        resource: String,
        source: kube::Error,
    },
}

impl TestClient {
    /// Create a new [`TestClient`] using either `KUBECONFIG` or the in-cluster environment
    /// variables.
    pub async fn new() -> Result<Self> {
        let k8s_client = kube::Client::try_default().await.context(Initialization)?;
        Ok(Self::new_from_k8s_client(k8s_client))
    }

    /// Create a new [`TestClient`] from an existing k8s client.
    pub fn new_from_k8s_client(k8s_client: kube::Client) -> Self {
        Self {
            api: Api::<Test>::namespaced(k8s_client, TESTSYS_NAMESPACE),
        }
    }

    /// Get the TestSys [`Test`].
    pub async fn get_test<S>(&self, name: S) -> Result<Test>
    where
        S: AsRef<str>,
    {
        Ok(self.api.get(name.as_ref()).await.context(KubeApiCall {
            method: "get",
            resource: "test",
        })?)
    }

    /// Get the TestSys [`Test`]'s `status.agent` field.
    pub async fn get_agent_status<S>(&self, name: S) -> Result<AgentStatus>
    where
        S: AsRef<str>,
    {
        Ok(self
            .get_test(name)
            .await?
            .status
            .unwrap_or_else(|| Default::default())
            .agent
            .unwrap_or_else(|| Default::default()))
    }

    /// Set the TestSys [`Test`]'s `status.agent` field. Returns the updated [`Test`].
    pub async fn set_agent_status<S>(&self, name: S, status: AgentStatus) -> Result<Test>
    where
        S: AsRef<str>,
    {
        let patch = Patch::Apply(json!({
            "apiVersion": TESTSYS_API,
            "kind": "Test",
            "status": {
                "agent": status
            }
        }));

        let ps = PatchParams::apply("TestClient").force();
        let updated_test = self
            .api
            .patch_status(name.as_ref(), &ps, &patch)
            .await
            .context(KubeApiCall {
                method: "patch",
                resource: "agent status",
            })?;
        Ok(updated_test)
    }

    /// Get a clone of the [`kube::Api`].
    pub fn api(&self) -> kube::Api<Test> {
        self.api.clone()
    }
}
