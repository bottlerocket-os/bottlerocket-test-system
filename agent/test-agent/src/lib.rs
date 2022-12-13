/*!

The `test-agent` library allows you do design custom tests for TestSys to run. You do this by
implementing the [`Runner`] trait, then handing these to an [`Agent`] object, which you then package
as a binary in a container to run in the cluster.

!*/

mod agent;
mod bootstrap;
pub mod error;
mod k8s_client;

pub use crate::agent::TestAgent;
use agent_common::secrets::{Result as SecretsResult, SecretData, SecretsReader};
use async_trait::async_trait;
pub use bootstrap::{BootstrapData, BootstrapError};
use error::InfoClientResult;
pub use k8s_client::ClientError;
use log::info;
use model::clients::TestClient;
pub use model::{Configuration, TestResults};
use model::{Outcome, SecretName, SecretType};
use std::collections::BTreeMap;
use std::fmt::{Debug, Display};
use std::path::PathBuf;
use tempfile::TempDir;

/// Information that a test [`Runner`] needs before it can begin a test.
#[derive(Debug, Clone)]
pub struct Spec<C: Configuration> {
    pub name: String,
    pub configuration: C,
    pub secrets: BTreeMap<SecretType, SecretName>,
    pub results_dir: PathBuf,
}

/// The `Runner` trait provides a wrapper for any testing modality. You must implement this trait
/// for your unique testing situation.
///
/// The [`TestAgent`] will call your implementation of the `Runner` trait as follows:
/// - `new` will be called to instantiate the object.
/// - `run` will be called to run the test(s).
/// - `terminate` will be called before the program exits.
///
/// You will also define a [`Configuration`] type to define data that your test needs when it
/// starts. This requires serialization and other common traits, but otherwise can be whatever
/// you want it to be. The serialized form of this struct is provided to k8s when an instance of the
/// TestSys Test CRD is created.
///
#[async_trait]
pub trait Runner<I: InfoClient>: Sized + Send {
    /// Input that you need to initialize your test run.
    type C: Configuration;

    /// The error type returned by this trait's functions.
    type E: Debug + Display + Send + Sync + 'static;

    /// Creates a new instance of the `Runner`.
    async fn new(spec: Spec<Self::C>, info_client: &I) -> Result<Self, Self::E>;

    /// Runs the test(s) and returns when they are done. If the tests cannot be completed, returns
    /// an error.
    async fn run(&mut self, info_client: &I) -> Result<TestResults, Self::E>;

    /// Rerun a failed test.
    async fn rerun_failed(
        &mut self,
        _prev_test_result: &TestResults,
        _info_client: &I,
    ) -> Result<TestResults, Self::E> {
        info!("Tried to rerun test, but no retry method was defined.");
        Ok(TestResults {
            outcome: Outcome::Fail,
            num_failed: 1,
            num_passed: 0,
            num_skipped: 0,
            other_info: Some("rerun_failed not defined".to_string()),
        })
    }

    /// Cleans up prior to program exit.
    async fn terminate(&mut self) -> Result<(), Self::E>;

    /// Get the key/value pairs of a Kubernetes generic/[opaque] secret.
    /// [opaque]: https://kubernetes.io/docs/concepts/configuration/secret/#opaque-secrets
    // TODO - it is hacky to put this here. create something like the resource agent's InfoClient
    fn get_secret(&self, secret_name: &SecretName) -> SecretsResult<SecretData> {
        let secrets_reader = SecretsReader::new();
        secrets_reader.get_secret(secret_name)
    }
}

/// The `Client` is an interface to the k8s TestSys Test CRD API. The purpose of the interface is to
/// allow injection of a mock for development and testing of test agents without the presence of a
/// k8s cluster. In practice you will use the provided implementation by calling
/// `DefaultClient::new()`.
#[async_trait]
pub trait Client: Sized {
    /// The error type returned by this trait's functions.
    type E: Debug + Display + Send + Sync + 'static;

    /// Create a new instance of the `Client`. The [`TestAgent`] will instantiate the `Client` with
    /// this function after it obtains `BootstrapData`.
    async fn new(bootstrap_data: BootstrapData) -> Result<Self, Self::E>;

    /// Get the information needed by a test [`Runner`] from the k8s API.
    async fn spec<C>(&self) -> Result<Spec<C>, Self::E>
    where
        C: Configuration;

    /// Get the directory that the test's results are stored in.
    async fn results_directory(&self) -> Result<PathBuf, Self::E>;

    /// Get the file that the test's tar results should be stored in.
    async fn results_file(&self) -> Result<PathBuf, Self::E>;

    /// Determine if the pod should keep running after it has finished or encountered and error.
    async fn keep_running(&self) -> Result<bool, Self::E>;

    /// Determine the number of retries the agent is expected to perform for failed tests.
    async fn retries(&self) -> Result<u32, Self::E>;

    /// Set the appropriate status field to represent that the test has started.
    async fn send_test_starting(&self) -> Result<(), Self::E>;

    /// Update the TestResults in the tests status.
    async fn send_test_update(&self, results: TestResults) -> Result<(), Self::E>;

    /// Add a TestResults object to the CRDs array of TestResults without signaling that the test
    /// is complete. This is used to send TestResults when some failures have occured and we are
    /// going to re-run the failed test cases.
    async fn send_test_results(&self, results: TestResults) -> Result<(), Self::E>;

    /// Send an error to the k8s API.
    async fn send_error<E>(&self, error: E) -> Result<(), Self::E>
    where
        E: Debug + Display + Send + Sync;

    /// Set the task state as `Completed` indicating that no more retries or testing will occur.
    async fn send_test_completed(&self) -> Result<(), Self::E>;
}

/// Provides the default [`Client`] implementation.
pub struct DefaultClient {
    client: TestClient,
    name: String,
    results_dir: TempDir,
}

#[async_trait::async_trait]
pub trait InfoClient: Sized + Send + Sync {
    async fn new(d: BootstrapData) -> InfoClientResult<Self>;
    async fn send_test_update(&self, results: TestResults) -> InfoClientResult<()>;
}

pub struct DefaultInfoClient {
    client: TestClient,
    data: BootstrapData,
}
