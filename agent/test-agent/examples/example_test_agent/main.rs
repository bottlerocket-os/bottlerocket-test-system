/*!

This is an example of how a test agent is implemented.
This example program needs to run in a pod in a K8s cluster containing all the testsys-related CRDs.
(See yamlgen/deploy/testsys.yaml)

All the program does is echo "hello" a custom number of times and with a time delay in between.
See `ExampleConfig` for the different configuration values.

To build the container for this example test agent, run `make example-test-agent` from the
root directory of this repository.

An example manifest for deploying the test definition for this test-agent to a K8s cluster:

```yaml
apiVersion: testsys.bottlerocket.aws/v1
kind: Test
metadata:
  name: hello-world
  namespace: testsys-bottlerocket-aws
spec:
  image: "<CONTAINER IMAGE URL>"
  configuration:
    person: Bones the Cat
    hello_count: 10
    hello_duration_seconds: 3
```

!*/

use async_trait::async_trait;
use model::TestResults;
use serde::{Deserialize, Serialize};
use std::process::{Child, Command};
use test_agent::{Configuration, RunnerStatus, TestInfo};

struct ExampleTestRunner {
    config: ExampleConfig,
    /// Track `Child` process here.
    process: Option<Child>,
}

/// When implementing an actual [`Runner`], you may need some input in order to start the test.
/// You would define that input in a struct which implements [`Configuration`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ExampleConfig {
    person: String,
    hello_count: u32,
    hello_duration_seconds: u32,
}

impl Configuration for ExampleConfig {}

#[async_trait]
impl test_agent::Runner for ExampleTestRunner {
    type C = ExampleConfig;
    type E = String;

    async fn new(test_info: TestInfo<Self::C>) -> Result<Self, Self::E> {
        println!("Initializing example testsys agent...");
        Ok(Self {
            config: test_info.configuration,
            process: None,
        })
    }

    async fn spawn(&mut self) -> Result<(), Self::E> {
        if self.process.is_some() {
            return Err("already spawned".into());
        }
        let loop_cmd = format!(
            r#"i=1; while [ "$i" -le {} ]; do echo "hello {}" && sleep {}; i=$(( i + 1 )); done"#,
            self.config.hello_count, self.config.person, self.config.hello_duration_seconds
        );

        // Start the hello loop in a child process.
        let child = Command::new("sh")
            .arg("-c")
            .arg(loop_cmd)
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
                    if let Err(e) = child.kill() {
                        eprintln!("unable to kill process: {}", e);
                    }
                }
                Err(e) => {
                    // If can't get child status, try to kill the child process anyways.
                    eprintln!("unable to get child status: {}", e);
                    if let Err(e) = child.kill() {
                        eprintln!("unable to kill process: {}", e);
                    }
                }
            }
        }
        self.process = None;
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let mut agent_main =
        test_agent::TestAgent::<test_agent::DefaultClient, ExampleTestRunner>::new(
            test_agent::DefaultBootstrap,
        )
        .await
        .unwrap();
    agent_main.run().await.unwrap();
}
