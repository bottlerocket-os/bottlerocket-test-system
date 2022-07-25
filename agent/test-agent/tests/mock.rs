/*!

The purpose of this test is to demonstrate the mocking of a [`Client`] and a [`Bootstrap`] in order
to test a [`Runner`] with the [`TestAgent`].

!*/

use async_trait::async_trait;
use model::{Configuration, Outcome};
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Display};
use std::path::PathBuf;
use tempfile::{tempdir, TempDir};
use test_agent::{BootstrapData, Client, Runner};
use test_agent::{Spec, TestResults};
use tokio::time::{sleep, Duration};

/// When creating a test, this is the object that you create which will implement the [`Runner`]
/// trait. In our case, `MyRunner` shells out to `sh` and `echo` hello a few times.
struct MyRunner {
    /// In an actual [`Runner`] you would probably want to hold this information, which is provided
    /// by `new`.
    _spec: Spec<MyConfig>,
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

    async fn new(spec: Spec<Self::C>) -> Result<Self, Self::E> {
        Ok(Self { _spec: spec })
    }

    async fn run(&mut self) -> Result<TestResults, Self::E> {
        println!("MyRunner::run");
        for i in 1..=5 {
            println!("Hello {}", i);
            sleep(Duration::from_millis(50)).await;
        }

        Ok(TestResults {
            outcome: Outcome::Pass,
            ..TestResults::default()
        })
    }

    async fn terminate(&mut self) -> Result<(), Self::E> {
        println!("MyRunner::terminate");
        Ok(())
    }
}

/// So that we do not need a running k8s system in order to test [`MyRunner`], we implement a mock
/// of [`Client`]. In this case it just prints out its function calls.
struct MockClient {
    results_dir: TempDir,
    results_file: TempDir,
}

#[async_trait]
impl Client for MockClient {
    /// We use a `String` as the error type for convenience.
    type E = String;

    async fn new(_: BootstrapData) -> Result<Self, Self::E> {
        Ok(Self {
            results_dir: tempdir().unwrap(),
            results_file: tempdir().unwrap(),
        })
    }

    async fn spec<C>(&self) -> Result<Spec<C>, Self::E>
    where
        C: Configuration,
    {
        println!("MockClient::get");
        Ok(Spec {
            name: "mock-test".into(),
            configuration: C::default(),
            secrets: Default::default(),
            results_dir: Default::default(),
        })
    }

    async fn send_test_starting(&self) -> Result<(), Self::E> {
        println!("MockClient::send_test_starting");
        Ok(())
    }

    async fn send_test_done(&self, results: TestResults) -> Result<(), Self::E> {
        println!("MockClient::send_test_done: {:?}", results);
        Ok(())
    }

    async fn send_test_results(&self, results: TestResults) -> Result<(), Self::E> {
        println!("MockClient::send_test_results: {:?}", results);
        Ok(())
    }

    async fn send_error<E>(&self, error: E) -> Result<(), Self::E>
    where
        E: Debug + Display + Send + Sync,
    {
        println!("MockClient::send_error {}", error);
        Ok(())
    }

    async fn keep_running(&self) -> Result<bool, Self::E> {
        Ok(false)
    }

    async fn results_directory(&self) -> Result<PathBuf, Self::E> {
        Ok(self.results_dir.path().to_path_buf())
    }

    async fn results_file(&self) -> Result<PathBuf, Self::E> {
        Ok(self.results_file.path().join("result.tar.gz"))
    }

    async fn retries(&self) -> Result<u32, Self::E> {
        Ok(0)
    }

    async fn send_test_completed(&self) -> Result<(), Self::E> {
        println!("MockClient::send_test_completed");
        Ok(())
    }
}

/// This test runs [`MyRunner`] inside a [`TestAgent`] with k8s and the container environment mocked
/// by `MockClient` and `MockBootstrap`.
#[tokio::test]
async fn mock_test() -> std::io::Result<()> {
    let mut agent_main = test_agent::TestAgent::<MockClient, MyRunner>::new(BootstrapData {
        test_name: String::from("hello-test"),
    })
    .await
    .unwrap();
    agent_main.run().await.unwrap();
    assert!(std::path::Path::new(&agent_main.results_file().await.unwrap()).is_file());
    Ok(())
}
