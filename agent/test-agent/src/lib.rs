mod agent;
pub(crate) mod constants;
pub mod error;
mod k8s_bootstrap;
mod k8s_client;

pub use agent::TestAgent;
use async_trait::async_trait;
pub use client::model::{Configuration, TestResults};
use client::TestClient;
pub use k8s_bootstrap::BootstrapError;
pub use k8s_client::ClientError;
use std::fmt::{Debug, Display};

/// The status of the test `Runner`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RunnerStatus {
    /// The test process has not encountered any fatal errors and is proceeding.
    Running,
    /// The test process has concluded (irrespective of tests passing or failing).
    Done(TestResults),
}

/// Information that a test [`Runner`] needs before it can begin a test.
#[derive(Debug, Clone)]
pub struct TestInfo<C: Configuration> {
    pub name: String,
    pub configuration: C,
}

/// The `Runner` trait provides a wrapper for any testing modality. You must implement this trait
/// for your unique testing situation.
///
/// The [`TestAgent`] will call your implementation of the `Runner` trait as follows:
/// - `new` will be called to instantiate the object.
/// - `spawn` will be called to begin the test process. You will run your test process in a separate
///   thread/process and return from `spawn` once the test has been kicked off.
/// - `status` will be repeatedly called after spawn until either an `Error` or `Done` is received.
/// - `terminate` will be called before the program exits.
///
/// You will also define a [`Configuration`] type to define data that your test needs when it
/// starts. This requires serialization and other common traits, but otherwise can be whatever
/// you want it to be. The serialized form of this struct is provided to k8s when an instance of the
/// TestSys Test CRD is created.
///
#[async_trait]
pub trait Runner: Sized {
    /// Input that you need to initialize your test run.
    type C: Configuration;

    /// The error type returned by this trait's functions.
    type E: Debug + Display + Send + Sync + 'static;

    /// Creates a new instance of the `Runner`.
    async fn new(test_info: TestInfo<Self::C>) -> Result<Self, Self::E>;

    /// Starts the testing process. The testing process should run in a separate thread or child
    /// process and `spawn` should return as soon as that process is running successfully. `spawn`
    /// has a timeout (see [`SPAWN_TIMEOUT`]).
    async fn spawn(&mut self) -> Result<(), Self::E>;

    /// Checks and returns the status of the test. If possible, `status` should check that the
    /// testing process has not failed and is still on track to finish successfully, in which case
    /// `status` should return [`Status::Running`]. If the testing process has completed (regardless
    /// of whether tests have passed or failed), `status` should return [`Status::Done`]. If the
    /// test process has encountered an error and cannot continue, `status` should return an error.
    /// `status` has a timeout (see [`STATUS_TIMEOUT`].
    async fn status(&mut self) -> Result<RunnerStatus, Self::E>;

    /// Cleans up prior to program exit. `terminate` can be called either because the testing has
    /// completed successfully, or because the test run has been cancelled. `terminate` has a
    /// timeout (see [`TERMINATE_TIMEOUT`]).
    async fn terminate(&mut self) -> Result<(), Self::E>;
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
    async fn get_test_info<C>(&self) -> Result<TestInfo<C>, Self::E>
    where
        C: Configuration;

    /// Send the test [`Runner`]'s status to the k8s API.
    async fn send_status(&self, status: RunnerStatus) -> Result<(), Self::E>;

    /// Check the k8s API to find out if this test run is cancelled.
    async fn is_cancelled(&self) -> Result<bool, Self::E>;

    /// Send an error to the k8s API.
    async fn send_error<E>(&self, error: E) -> Result<(), Self::E>
    where
        E: Debug + Display + Send + Sync;
}

/// Provides the default [`Client`] implementation.
pub struct DefaultClient {
    client: TestClient,
    name: String,
}

/// The `Bootstrap` trait provides the information needed by the test agent before a k8s client can
/// be instantiated. For example, if some data such as the test name is provided by way of the k8s
/// downward API or ConfigMaps, the `Bootstrap` trait will provide that  information. It is offered
/// as a trait to enable testing of a [`Runner`] outside of a k8s pod. In practice you will use the
/// provided implementation by calling `DefaultBootstrap::new()`.
#[async_trait]
pub trait Bootstrap: Sized {
    type E: Debug + Display + Send + Sync + 'static;

    /// Reads data from the container's environment, filesystem, etc. and provides that information.
    async fn read(&self) -> Result<BootstrapData, Self::E>;
}

/// Data that is read from the TestPod's container environment and filesystem.
pub struct BootstrapData {
    /// The name of the TestSys Test.
    pub test_name: String,
}

/// Provides the default [`Bootstrap`] implementation.
pub struct DefaultBootstrap;
