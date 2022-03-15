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
    hello_duration_milliseconds: 3
```

!*/

use async_trait::async_trait;
use model::{Outcome, TestResults};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use test_agent::{BootstrapData, Configuration, Spec};
use tokio::time::{sleep, Duration};

struct ExampleTestRunner {
    config: ExampleConfig,
}

/// When implementing an actual [`Runner`], you may need some input in order to start the test.
/// You would define that input in a struct which implements [`Configuration`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExampleConfig {
    person: String,
    hello_count: u32,
    hello_duration_milliseconds: u32,
    nested: Option<Nested>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Nested {
    data: Value,
}

impl Configuration for ExampleConfig {}

#[async_trait]
impl test_agent::Runner for ExampleTestRunner {
    type C = ExampleConfig;
    type E = String;

    async fn new(spec: Spec<Self::C>) -> Result<Self, Self::E> {
        println!("Initializing example testsys agent...");
        Ok(Self {
            config: spec.configuration,
        })
    }

    async fn run(&mut self) -> Result<TestResults, Self::E> {
        println!("ExampleTestRunner::run");
        for i in 1..=self.config.hello_count {
            println!("hello #{} to {}", i, self.config.person);
            sleep(Duration::from_millis(
                self.config.hello_duration_milliseconds.into(),
            ))
            .await
        }
        if let Some(nested) = &self.config.nested {
            println!("Nested Data:\n {:?}", nested.data);
        }
        Ok(TestResults {
            outcome: Outcome::Fail,
            ..TestResults::default()
        })
    }

    async fn rerun_failed(&mut self, _prev_result: &TestResults) -> Result<TestResults, Self::E> {
        println!("ExampleTestRunner::run");
        for i in 1..=self.config.hello_count {
            println!("hello #{} to {}", i, self.config.person);
            sleep(Duration::from_millis(
                self.config.hello_duration_milliseconds.into(),
            ))
            .await
        }
        if let Some(nested) = &self.config.nested {
            println!("Nested Data:\n {:?}", nested.data);
        }
        Ok(TestResults {
            outcome: Outcome::Pass,
            ..TestResults::default()
        })
    }

    async fn terminate(&mut self) -> Result<(), Self::E> {
        println!("ExampleTestRunner::terminate");
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let mut agent_main =
        test_agent::TestAgent::<test_agent::DefaultClient, ExampleTestRunner>::new(
            BootstrapData::from_env().unwrap(),
        )
        .await
        .unwrap();
    agent_main.run().await.unwrap();
}
