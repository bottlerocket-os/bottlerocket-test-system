use crate::model::{AgentStatus, ControllerStatus, Test, TESTSYS, TESTSYS_API, TESTSYS_NAMESPACE};
use kube::api::{Patch, PatchParams};
use kube::{Api, Resource};
use log::trace;
use serde::Serialize;
use serde_json::{json, Value};
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

    #[snafu(display("Unable to {} {}: {}", method, what, source))]
    KubeApiCall {
        method: String,
        what: String,
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
            what: "test",
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
        let json = Self::create_patch("status", "agent", status);
        self.patch_status(&name, &json, "agent status").await
    }

    /// Set the TestSys [`Test`]'s `status.controller` field. Returns the updated [`Test`].
    pub async fn set_controller_status<S>(&self, name: S, status: ControllerStatus) -> Result<Test>
    where
        S: AsRef<str>,
    {
        let json = Self::create_patch("status", "controller", status);
        self.patch_status(&name, &json, "controller status").await
    }

    /// Add a k8s 'finalizer' to the [`Test`] object's metadata.
    pub async fn add_finalizer<S1, S2>(&self, test_name: S1, finalizer_name: S2) -> Result<Test>
    where
        S1: AsRef<str>,
        S2: AsRef<str>,
    {
        let finalizer = Self::create_finalizer(finalizer_name);
        let mut finalizers: Vec<String> = self
            .get_test(&test_name)
            .await?
            .meta()
            .finalizers
            .to_owned();
        finalizers.push(finalizer);
        let json = Self::create_patch("metadata", "finalizers", &finalizers);
        let patch: Patch<&Value> = Patch::Merge(&json);
        self.patch(&test_name, patch, "finalizers").await
    }

    pub async fn remove_finalizer<S1, S2>(&self, test_name: S1, finalizer_name: S2) -> Result<Test>
    where
        S1: AsRef<str>,
        S2: AsRef<str>,
    {
        let finalizer = Self::create_finalizer(&finalizer_name);
        trace!("removing finalizer: {}", finalizer);
        let mut finalizers: Vec<String> = self
            .get_test(&test_name)
            .await?
            .meta()
            .finalizers
            .to_owned();
        finalizers.retain(|item| item.as_str() != finalizer.as_str());
        let json = Self::create_patch("metadata", "finalizers", &finalizers);
        let patch: Patch<&Value> = Patch::Merge(&json);
        self.patch(&test_name, patch, "finalizers").await
    }

    /// Helper for finding a given finalizer in a `Test` object.
    pub fn has_finalizer<S: AsRef<str>>(test: &Test, finalizer_name: S) -> bool {
        test.meta()
            .finalizers
            .contains(&Self::create_finalizer(finalizer_name))
    }

    /// Get a clone of the [`kube::Api`].
    pub fn api(&self) -> kube::Api<Test> {
        self.api.clone()
    }

    /// Private helper to create a domain-like finalizer name. For example, given "foo", returns
    /// "foo.finalizers.bottlerocket.aws".
    fn create_finalizer<S: AsRef<str>>(finalizer_name: S) -> String {
        format!("{}.finalizers.{}", finalizer_name.as_ref(), TESTSYS)
    }

    /// Creates the JSON object used in an HTTP PATCH operation.
    fn create_patch<K1, K2, T>(top_key: K1, sub_key: K2, value: T) -> Value
    where
        K1: AsRef<str>,
        K2: AsRef<str>,
        T: Serialize,
    {
        json!({
            "apiVersion": TESTSYS_API,
            "kind": "Test",
            top_key.as_ref(): {
                sub_key.as_ref(): value
            }
        })
    }

    /// Applies a change to a `Test` object with an HTTP PATCH operation.
    async fn patch<S>(&self, test_name: S, patch: Patch<&Value>, what: &str) -> Result<Test>
    where
        S: AsRef<str>,
    {
        Ok(self
            .api
            .patch(test_name.as_ref(), &PatchParams::default(), &patch)
            .await
            .context(KubeApiCall {
                method: "patch",
                what,
            })?)
    }

    /// Applies a change to the `status` field of a `Test` object with an HTTP PATCH operation.
    async fn patch_status<S>(&self, test_name: S, json: &Value, what: &str) -> Result<Test>
    where
        S: AsRef<str>,
    {
        let ps = PatchParams::apply("TestClient").force();
        Ok(self
            .api
            .patch_status(test_name.as_ref(), &ps, &Patch::Apply(json))
            .await
            .context(KubeApiCall {
                method: "patch",
                what,
            })?)
    }
}
