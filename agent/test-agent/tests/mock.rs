/*!

The purpose of this test is to demonstrate the mocking of a [`Client`] and a [`Bootstrap`] in order
to test a [`Runner`] with the [`TestAgent`].

!*/

use async_trait::async_trait;
use model::Configuration;
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Display};
use test_agent::{Bootstrap, BootstrapData, Client, Runner};
use test_agent::{RunnerStatus, TestInfo, TestResults};
use tokio::process::{Child, Command};

/// When creating a test, this is the object that you create which will implement the [`Runner`]
/// trait. In our case, `MyRunner` shells out to `sh` and `echo` hello a few times.
struct MyRunner {
    /// In an actual [`Runner`] you would probably want to hold this information, which is provided
    /// by `new`.
    _info: TestInfo<MyConfig>,
    /// When we spawn our hello loop, we will hold the `Child` process here.
    process: Option<Child>,
}

/// When implementing an actual [`Runner`], you may need some input in order to start the test.
/// You would define that input in a struct which implements [`Configuration`].
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
struct MyConfig {}

impl Configuration for MyConfig {}

#[async_trait]
impl Runner for MyRunner {
    /// The configuration type we defined above.
    type C = MyConfig;

    /// The error type. In this case we use a `String`, but you can use a real error type.
    type E = String;

    async fn new(info: TestInfo<Self::C>) -> Result<Self, Self::E> {
        Ok(Self {
            _info: info,
            process: None,
        })
    }

    async fn spawn(&mut self) -> Result<(), Self::E> {
        if self.process.is_some() {
            return Err("already spawned".into());
        }

        // Start the hello loop in a child process.
        let child = Command::new("sh")
            .arg("-c")
            .arg(r#"i=1; while [ "$i" -le 5 ]; do echo "hello" && sleep 0.1; i=$(( i + 1 )); done"#)
            .spawn()
            .map_err(|e| format!("{}", e))?;

        // Hold on to the child process and return from this function.
        self.process = Some(child);
        Ok(())
    }

    async fn status(&mut self) -> Result<RunnerStatus, Self::E> {
        // Unwrap the child process from its option.
        let child = if let Some(process) = &mut self.process {
            process
        } else {
            // Status will never be called before spawn has returned.
            return Err("process not spawned".into());
        };

        // Check the child process. If it has completed, return `Done`,
        // otherwise return `Running`.
        if let Some(_exit) = child
            .try_wait()
            .map_err(|e| format!("unable to check status: {}", e))?
        {
            Ok(RunnerStatus::Done(TestResults::default()))
        } else {
            Ok(RunnerStatus::Running)
        }
    }

    async fn terminate(&mut self) -> Result<(), Self::E> {
        if let Some(child) = &mut self.process {
            match child.try_wait() {
                Ok(Some(status)) => {
                    // Child already exited
                    println!("child already exited with {}", status);
                    return Ok(());
                }
                Ok(None) => {
                    // Child process is still running. In a real
                    // test scenario, you might first want to send SIGTERM and wait.
                    // You may also have resources to clean up.
                    if let Err(e) = child.kill().await {
                        eprintln!("unable to kill process: {}", e);
                    }
                }
                Err(e) => {
                    // If can't get child status, try to kill the child process anyways.
                    eprintln!("unable to get child status: {}", e);
                    if let Err(e) = child.kill().await {
                        eprintln!("unable to kill process: {}", e);
                    }
                }
            }
        }
        self.process = None;
        Ok(())
    }
}

/// So that we do not need a running k8s system in order to test [`MyRunner`], we implement a mock
/// of [`Client`]. In this case it just prints out its function calls.
struct MockClient {}

#[async_trait]
impl Client for MockClient {
    /// We use a `String` as the error type for convenience.
    type E = String;

    async fn new(_: BootstrapData) -> Result<Self, Self::E> {
        Ok(Self {})
    }

    async fn get_test_info<C>(&self) -> Result<TestInfo<C>, Self::E>
    where
        C: Configuration,
    {
        println!("client: get");
        Ok(TestInfo {
            name: "mock-test".into(),
            configuration: C::default(),
        })
    }

    async fn send_status(&self, status: RunnerStatus) -> Result<(), Self::E> {
        println!("client: send status {:?}", status);
        Ok(())
    }

    async fn is_cancelled(&self) -> Result<bool, Self::E> {
        Ok(false)
    }

    async fn send_error<E>(&self, error: E) -> Result<(), Self::E>
    where
        E: Debug + Display + Send + Sync,
    {
        println!("client: send error {}", error);
        Ok(())
    }
}

/// So that we can test [`MyRunner`] without placing it into an k8s pod with the correct environment
/// variables and filesystem structure, we mock out the [`Bootstrap`] trait.
struct MockBootstrap {}

#[async_trait]
impl Bootstrap for MockBootstrap {
    /// We use a `String` as the error type for convenience.
    type E = String;

    async fn read(&self) -> Result<BootstrapData, Self::E> {
        Ok(BootstrapData {
            test_name: "hello-world".to_string(),
        })
    }
}

/// This test runs [`MyRunner`] inside a [`TestAgent`] with k8s and the container environment mocked
/// by `MockClient` and `MockBootstrap`.
#[tokio::test]
async fn mock_test() -> std::io::Result<()> {
    let mut agent_main = test_agent::TestAgent::<MockClient, MyRunner>::new(MockBootstrap {})
        .await
        .unwrap();
    agent_main.run().await.unwrap();
    Ok(())
}
