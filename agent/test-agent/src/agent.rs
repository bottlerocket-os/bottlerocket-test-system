use crate::error::{self, AgentError, Error, Result};
use crate::{BootstrapData, Client, Runner};
use log::{debug, error};
use snafu::ResultExt;
use std::fs::File;
use std::path::PathBuf;
use std::time::Duration;
use tar::Builder;

/// The `TestAgent` is the main entrypoint for the program running in a TestPod. It starts a test
/// run, regularly checks the health of the test run, observes cancellation of a test run, and sends
/// the results of a test run.
///
/// To create a test, implement the [`Runner`] trait on an object and inject it into the
/// `TestAgent`.
///
/// Two additional dependencies are injected for the sake of testability. You can mock these traits
/// in order to test your [`Runner`] in the absence of k8s.
/// - [`Bootstrap`] collects information from the container environment.
/// - [`Client`] communicates with the k8s server.
///
/// See the `../examples/example_test_agent/main.rs` for an example of how to create a [`Runner`].
/// Also see `../tests/mock.rs` for an example of how you can mock the Kubernetes clients.
///
pub struct TestAgent<C, R>
where
    C: Client + 'static,
    R: Runner + 'static,
{
    client: C,
    runner: R,
}

impl<C, R> TestAgent<C, R>
where
    C: Client + 'static,
    R: Runner + 'static,
{
    /// Create a new `TestAgent`. Since the [`Client`] and [`Runner`] are constructed internally
    /// based on information from the [`BootstrapData`], you will need to specify the types using
    /// the type parameters. `TestAgent::<DefaultClient, MyRunner>::new(BootstrapData::from_env())`.
    /// Any errors that occur during this function are fatal since we are not able to fully
    /// construct the `Runner`.
    pub async fn new(b: BootstrapData) -> Result<Self, C::E, R::E> {
        let client = C::new(b).await.map_err(Error::Client)?;
        let spec = client.spec().await.map_err(Error::Client)?;
        let runner = R::new(spec).await.map_err(Error::Runner)?;
        Ok(Self { runner, client })
    }

    /// Run the `TestAgent`. This function returns once the test has completed and `keep_running`
    /// is `false`.
    pub async fn run(&mut self) -> Result<(), C::E, R::E> {
        let result = self.run_inner().await;
        let tar_result = self.tar_results().await;

        loop {
            let keep_running = match self.client.keep_running().await {
                Err(e) => {
                    error!("Unable to communicate with Kuberenetes: '{}'", e);
                    // If we can't communicate Kubernetes, its safest to
                    // stay running in case some debugging is needed.
                    true
                }
                Ok(value) => value,
            };
            if !keep_running {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2000)).await;
        }
        // We want the running error first if there was one.
        match result {
            Err(e) => Err(e),
            Ok(()) => tar_result,
        }
    }

    /// Run the `TestAgent`. This function returns once the test has completed.
    async fn run_inner(&mut self) -> Result<(), C::E, R::E> {
        debug!("running test");
        self.client
            .send_test_starting()
            .await
            .map_err(error::Error::Client)?;

        let test_results = match self.runner.run().await.map_err(error::Error::Runner) {
            Ok(ok) => ok,
            Err(e) => {
                self.send_error_best_effort(&e).await;
                self.terminate_best_effort().await;
                return Err(e);
            }
        };

        if let Err(e) = self
            .client
            .send_test_done(test_results)
            .await
            .map_err(error::Error::Client)
        {
            self.send_error_best_effort(&e).await;
            self.terminate_best_effort().await;
            return Err(e);
        }

        // Test finished successfully. Try to terminate. If termination fails, we try to send the
        // error to k8s, and return the error so that the process will exit with error.
        if let Err(e) = self.runner.terminate().await.map_err(error::Error::Runner) {
            error!("unable to terminate test runner: {}", e);
            self.send_error_best_effort(&e).await;
            return Err(e);
        }

        Ok(())
    }

    /// Returns `true` if the error was successfully sent, `false` if the error could not be sent.
    async fn send_error_best_effort(&mut self, e: &Error<C::E, R::E>) {
        if let Err(send_error) = self.client.send_error(e).await {
            error!(
                "unable to send error message '{}' to k8s: {}",
                e, send_error
            );
        }
    }

    /// Tells the `Runner` to terminate. If an error occurs, tries to send it to k8s, but logs it
    /// if it cannot be sent to k8s.
    async fn terminate_best_effort(&mut self) {
        // TODO - stay running https://github.com/bottlerocket-os/bottlerocket-test-system/issues/79
        if let Err(e) = self.runner.terminate().await.map_err(error::Error::Runner) {
            self.send_error_best_effort(&e).await;
        }
    }

    /// Converts the provided directory to a tar saved to `TESTSYS_RESULTS`.
    async fn tar_results(&mut self) -> Result<(), C::E, R::E> {
        let results_dir = self
            .client
            .results_directory()
            .await
            .map_err(Error::Client)?;

        let tar = File::create(self.client.results_file().await.map_err(Error::Client)?)
            .context(error::Archive)
            .map_err(|e| Error::Agent(AgentError::from(e)))?;
        let mut archive = Builder::new(tar);
        archive
            .append_dir_all("test-results", results_dir)
            .context(error::Archive)
            .map_err(|e| Error::Agent(AgentError::from(e)))?;
        archive
            .into_inner()
            .context(error::Archive)
            .map_err(|e| Error::Agent(AgentError::from(e)))?;
        Ok(())
    }

    pub async fn results_file(&self) -> Result<PathBuf, C::E, R::E> {
        self.client.results_file().await.map_err(Error::Client)
    }
}
