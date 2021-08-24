use crate::constants::{SPAWN_TIMEOUT, STATUS_CHECK_WAIT, STATUS_TIMEOUT, TERMINATE_TIMEOUT};
use crate::error::{self, Error, InnerError, Result};
use crate::{Bootstrap, Client, Runner, RunnerStatus};
use log::{debug, error};
use snafu::ResultExt;
use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;
use tokio::time::timeout as tokio_timeout;

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
// TODO - link to example https://github.com/bottlerocket-os/bottlerocket-test-system/issues/8
// TODO - for now see mock https://github.com/bottlerocket-os/bottlerocket-test-system/pull/63
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
    /// based on information from the [`Bootstrap`], you will need to specify the types using the
    /// type parameters. `TestAgent::<DefaultClient, MyRunner>::new(DefaultBootstrap::new())`. Any
    /// errors that occur during this function are fatal since we are not able to fully construct
    /// the `Runner`.
    pub async fn new<B>(bootstrap: B) -> Result<Self, C::E, R::E>
    where
        B: Bootstrap,
    {
        let bootstrap_data = bootstrap.read().await.map_err(|e| InnerError::Bootstrap {
            error: e.to_string(),
        })?;
        let client = C::new(bootstrap_data).await.map_err(|e| Error::Client(e))?;
        let test_info = client.get_test_info().await.map_err(|e| Error::Client(e))?;
        let runner = R::new(test_info).await.map_err(|e| Error::Runner(e))?;
        Ok(Self { runner, client })
    }

    /// Run the `TestAgent`. This function returns once the test has completed or been cancelled.
    pub async fn run(&mut self) -> Result<(), C::E, R::E> {
        // Run the spawn function. If an error occurs, attempt to allow the runner to terminate and
        // attempt to send the error to k8s. Return the original spawn error.
        debug!("spawning test");
        let spawn_fut = self.runner.spawn();
        let spawn_result = Self::wait(SPAWN_TIMEOUT, spawn_fut, "spawn").await;
        if let Err(spawn_err) = spawn_result {
            self.send_error_best_effort(&spawn_err).await;
            self.terminate_best_effort().await;
            return Err(spawn_err);
        }

        // Loop until the test is done or an error occurs.
        if let Err(e) = self.run_status_loop().await {
            error!("error during test run: {}", e);
            self.send_error_best_effort(&e).await;
            self.terminate_best_effort().await;
            return Err(e);
        }

        // Test finished successfully. Try to terminate. If termination fails, we try to send the
        // error to k8s, and return the error so that the process will exit with error.
        let terminate_fut = Self::wait(TERMINATE_TIMEOUT, self.runner.terminate(), "terminate");
        if let Err(e) = terminate_fut.await {
            error!("unable to terminate test runner: {}", e);
            self.send_error_best_effort(&e).await;
            return Err(e);
        }

        Ok(())
    }

    /// Loops checking status and returning when the status is either `Done` or an error occurs.
    /// This function updates k8s with the runner's status and test results.
    async fn run_status_loop(&mut self) -> Result<(), C::E, R::E> {
        loop {
            // Pause before the first, and each subsequent status check.
            sleep(STATUS_CHECK_WAIT).await;

            // A failed status check is fatal to the test run.
            // TODO - consider allowing multiple status check failures before returning an error
            let status = self.check_status().await?;

            // If sending status or test results to k8s fails it is fatal to the test.
            self.client
                .send_status(status.clone())
                .await
                .map_err(|e| Error::Client(e))?;

            // If status is Done, or if we have been canceled, we can stop looping and return.
            if matches!(status, RunnerStatus::Done(_)) {
                break;
            } else {
                // Failure to check our cancellation status is not fatal, so we log any error.
                match self.client.is_cancelled().await {
                    Err(e) => {
                        error!("unable to check cancellation status: {}", e);
                        // We do not know if we are cancelled, continue looping.
                    }
                    Ok(cancelled) if cancelled => {
                        // The test run has been cancelled, stop looping.
                        break;
                    }
                    _ => { /* continue looping */ }
                }
            }
        }
        Ok(())
    }

    /// Checks the status with a timeout.
    async fn check_status(&mut self) -> Result<RunnerStatus, C::E, R::E> {
        let status_fut = self.runner.status();
        Self::wait(STATUS_TIMEOUT, status_fut, "status").await
    }

    async fn wait<F, T>(timeout: Duration, future: F, op: &str) -> Result<T, C::E, R::E>
    where
        F: Future<Output = std::result::Result<T, R::E>>,
    {
        tokio_timeout(timeout, future)
            .await
            .context(error::Timeout {
                duration: timeout,
                op,
            })?
            .map_err(|e| Error::Runner(e))
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
        let terminate_fut = Self::wait(TERMINATE_TIMEOUT, self.runner.terminate(), "terminate");
        if let Err(e) = terminate_fut.await {
            self.send_error_best_effort(&e).await;
        }
    }
}
