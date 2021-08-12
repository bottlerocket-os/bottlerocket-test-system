use crate::error::{self, Result};
use client::model::{ControllerStatus, Test, TestStatus};
use client::TestClient;
use kube::{Api, Client, Resource};
use log::trace;
use snafu::ResultExt;
use std::borrow::Cow;

const MAIN_FINALIZER: &str = "owned";
const POD_FINALIZER: &str = "test-pod";

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
    /// Get a clone of `kube::Api<Test>`
    pub(crate) fn api(&self) -> Api<Test> {
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

    /// Return either a reference to the `ControllerStatus`, or an owned, default-constructed
    /// `ControllerStatus` if it did not already exist.
    pub(crate) fn controller_status(&self) -> Cow<'_, ControllerStatus> {
        let status: &TestStatus = match self.test.status.as_ref() {
            Some(status) => status,
            None => return Cow::Owned(ControllerStatus::default()),
        };

        match &status.controller {
            None => Cow::Owned(ControllerStatus::default()),
            Some(status) => Cow::Borrowed(status),
        }
    }

    /// Set the `Test` CRD's `status.controller` field.
    pub(crate) async fn set_controller_status(&mut self, status: ControllerStatus) -> Result<()> {
        let updated_test = self
            .test_client()
            .set_controller_status(self.name(), status)
            .await
            .context(error::SetControllerStatus {
                test_name: self.name(),
            })?;
        self.test = updated_test;
        Ok(())
    }

    /// Mark the TestSys `Test` as owned and controlled by this controller.
    pub(crate) async fn add_main_finalizer(&mut self) -> Result<()> {
        trace!("Adding main finalizer for test '{}'", self.name());
        self.add_finalizer(MAIN_FINALIZER).await
    }

    /// Mark the TestSys `Test` as having a test pod running that needs to be cleaned up before
    /// the `Test` can be deleted.
    pub(crate) async fn add_pod_finalizer(&mut self) -> Result<()> {
        trace!("Adding pod finalizer for test '{}'", self.name());
        self.add_finalizer(POD_FINALIZER).await
    }

    /// Whether the test has one or more finalizers.
    pub(crate) fn has_finalizers(&self) -> bool {
        !self.test.meta().finalizers.is_empty()
    }

    /// Whether the test has a finalizer representing the test pod.
    pub(crate) fn has_pod_finalizer(&self) -> bool {
        TestClient::has_finalizer(&self.test, POD_FINALIZER)
    }

    /// Remove the main finalizer to indicate that the controller is no longer managing the TestSys
    /// `Test` object so that k8s can delete it.
    pub(crate) async fn remove_main_finalizer(&mut self) -> Result<()> {
        trace!("Removing main finalizer for test '{}'", self.name());
        self.remove_finalizer(MAIN_FINALIZER).await
    }

    /// Remove the pod finalizer to indicate that the controller is no longer managing a test pod.
    pub(crate) async fn remove_pod_finalizer(&mut self) -> Result<()> {
        trace!("Removing pod finalizer for test '{}'", self.name());
        self.remove_finalizer(POD_FINALIZER).await
    }

    /// Whether or not someone has requested that k8s delete the TestSys `Test`.
    pub(crate) fn is_delete_requested(&self) -> bool {
        self.test.meta().deletion_timestamp.is_some()
    }

    /// Access the inner `TestClient` object with fewer keystrokes.
    fn test_client(&self) -> &TestClient {
        &self.context.get_ref().test_client
    }

    /// Add a finalizer and update the cached test.
    async fn add_finalizer(&mut self, finalizer_name: &str) -> Result<()> {
        let updated_test = self
            .test_client()
            .add_finalizer(self.name(), finalizer_name)
            .await
            .context(error::AddFinalizer {
                test_name: self.name(),
                finalizer: finalizer_name,
            })?;
        self.test = updated_test;
        trace!(
            "Added finalizer '{}' to test '{}': {}",
            finalizer_name,
            self.name(),
            self.test.meta().finalizers.join(", ")
        );
        Ok(())
    }

    /// Remove a finalizer and update the cached test.
    async fn remove_finalizer(&mut self, finalizer_name: &str) -> Result<()> {
        let updated_test = self
            .test_client()
            .remove_finalizer(self.name(), finalizer_name)
            .await
            .context(error::RemoveFinalizer {
                test_name: self.name(),
                finalizer: finalizer_name,
            })?;
        self.test = updated_test;
        trace!(
            "Removed finalizer '{}' from test '{}': {}",
            finalizer_name,
            self.name(),
            self.test.meta().finalizers.join(", ")
        );
        Ok(())
    }
}
