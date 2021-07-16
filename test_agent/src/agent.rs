use crate::error::{self, Error, InnerError, Result};
use crate::{
    Bootstrap, Client, Runner, Status, SPAWN_TIMEOUT, STATUS_CHECK_WAIT, STATUS_TIMEOUT,
    TERMINATE_TIMEOUT,
};
use snafu::ResultExt;
use tokio::time::{sleep, timeout};

/// The `TestAgent` is the main entrypoint for the program running in a TestPod. It starts a test
/// run, regularly checks the health of the test run, observes cancellation of a test run, and sends
/// the results of a test run.
///
/// The create a test, implement the [`Runner`] trait on an object and inject it into the
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
    /// type parameters. `TestAgent::<DefaultClient, MyRunner>::new(DefaultBootstrap::new())`.
    pub async fn new<B>(bootstrap: B) -> Result<Self, C::E, R::E>
    where
        B: Bootstrap,
    {
        let bootstrap_data = bootstrap.read().await.map_err(|e| InnerError::Bootstrap {
            error: e.to_string(),
        })?;
        let client = C::new(bootstrap_data).await.map_err(|e| Error::Client(e))?;
        let test_info = client.get().await.map_err(|e| Error::Client(e))?;
        let runner = R::new(test_info).await.map_err(|e| Error::Runner(e))?;
        Ok(Self { runner, client })
    }

    /// Run the `TestAgent`. This function returns once the test has completed or been cancelled.
    pub async fn run(&mut self) -> Result<(), C::E, R::E> {
        if let Err(e) = self.start_test_and_loop().await {
            if let Err(inner_error) = self.client.send_error(e).await {
                eprintln!("unable to send error to kubernetes: {}", inner_error);
            }
        }
        let terminate_fut = self.runner.terminate();
        timeout(TERMINATE_TIMEOUT, terminate_fut)
            .await
            .context(error::Timeout {
                duration: TERMINATE_TIMEOUT,
                op: "test termination",
            })?
            .map_err(|e| Error::Runner(e))
    }

    /// A convenience wrapper around the starting and health checking of the runner. This wrapper
    /// simplifies error handling in the `run` function.
    async fn start_test_and_loop(&mut self) -> Result<(), C::E, R::E> {
        let spawn_fut = self.runner.spawn();
        timeout(SPAWN_TIMEOUT, spawn_fut)
            .await
            .context(error::Timeout {
                duration: SPAWN_TIMEOUT,
                op: "test start",
            })?
            .map_err(|e| Error::Runner(e))?;
        self.status_loop().await
    }

    /// Loops checking status and returning when the status is either `Done` or an error occurs.
    async fn status_loop(&mut self) -> Result<(), C::E, R::E> {
        loop {
            sleep(STATUS_CHECK_WAIT).await;
            let status = self.check_status_with_timeout().await?;
            let done = matches!(status, Status::Done(_));
            self.client
                .send_status(status)
                .await
                .map_err(|e| Error::Client(e))?;
            if done {
                break;
            }
        }
        Ok(())
    }

    /// Checks the status with a timeout.
    async fn check_status_with_timeout(&mut self) -> Result<Status, C::E, R::E> {
        let status_fut = self.runner.status();
        timeout(STATUS_TIMEOUT, status_fut)
            .await
            .context(error::Timeout {
                duration: STATUS_TIMEOUT,
                op: "test status check",
            })?
            .map_err(|e| Error::Runner(e))
    }
}
