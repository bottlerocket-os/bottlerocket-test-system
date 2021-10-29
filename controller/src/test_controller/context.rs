use crate::error::Result;
use crate::job::{delete_job, get_job_state, JobState};
use anyhow::Context as AnyhowContext;
use kube::{Api, Client};
use model::clients::{CrdClient, TestClient};
use model::Test;

/// This is used by `kube-runtime` to pass any custom information we need when [`reconcile`] is
/// called.
pub(crate) type Context = kube_runtime::controller::Context<ContextData>;

pub(crate) fn new_context(client: Client) -> Context {
    kube_runtime::controller::Context::new(ContextData {
        test_client: TestClient::new_from_k8s_client(client),
    })
}

/// This type is wrapped by [`kube::Context`] and contains information we need during [`reconcile`].
#[derive(Clone)]
pub(crate) struct ContextData {
    test_client: TestClient,
}

impl ContextData {
    pub(crate) fn api(&self) -> &Api<Test> {
        self.test_client.api()
    }
}

/// The [`reconcile`] function has [`Test`] and [`Context`] as its inputs. For convenience, we
/// combine these and provide accessor and helper functions.
pub(crate) struct TestInterface {
    /// The cached [`Test`] object.
    test: Test,
    context: Context,
}

impl TestInterface {
    /// Create a new `TestInterface` from the [`Test`] and [`Context`].
    pub(crate) fn new(test: Test, context: Context) -> Result<Self> {
        Ok(Self { test, context })
    }

    /// Get the name of the test. In the `Test` struct the name field is optional, but in practice
    /// the name is required. We return a default zero length string in the essentially impossible
    /// `None` case instead of returning an `Option` or `Error`.
    pub(crate) fn name(&self) -> &str {
        self.test
            .metadata
            .name
            .as_ref()
            .map_or("", |value| value.as_str())
    }

    pub(crate) fn test(&self) -> &Test {
        &self.test
    }

    pub(crate) fn k8s_client(&self) -> kube::Client {
        self.api().clone().into_client()
    }

    pub(crate) fn api(&self) -> &Api<Test> {
        self.context.get_ref().api()
    }

    /// Access the inner `TestClient` object with fewer keystrokes.
    pub(super) fn test_client(&self) -> &TestClient {
        &self.context.get_ref().test_client
    }

    pub(super) async fn get_job_state(&self) -> Result<JobState> {
        get_job_state(self.k8s_client(), self.name())
            .await
            .with_context(|| format!("Unable to get job state for test '{}'", self.name()))
    }

    pub(super) async fn delete_job(&self) -> Result<()> {
        delete_job(self.k8s_client(), self.name())
            .await
            .with_context(|| format!("Unable to delete job for test '{}'", self.name()))
    }
}
