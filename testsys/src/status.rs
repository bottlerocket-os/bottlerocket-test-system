use crate::error::{self, Result};
use kube::Client;
use model::clients::TestClient;
use model::{Lifecycle, RunState, Test};
use serde::{Deserialize, Serialize};
use snafu::ResultExt;
use std::collections::HashMap;
use std::fmt::Display;
use structopt::StructOpt;

/// Check the status of a TestSys test.
#[derive(Debug, StructOpt)]
pub(crate) struct Status {
    /// Check the status of `Test` named `test_name`. Omit to check the status of all tests.
    #[structopt(long = "test-name", short = "t")]
    test_name: Option<String>,

    /// Continue checking the status of the test(s) until all have completed.
    #[structopt(long = "wait")]
    wait: bool,

    /// Output the results in JSON format.
    #[structopt(long = "json")]
    json: bool,
}

impl Status {
    pub(crate) async fn run(&self, k8s_client: Client) -> Result<()> {
        let tests_api = TestClient::new_from_k8s_client(k8s_client);
        let mut failures;
        let mut status_results;
        loop {
            failures = Vec::new();
            status_results = StatusResults::new();
            let tests = match self.test_name.as_ref() {
                Some(test_name) => vec![tests_api
                    .get_test(test_name)
                    .await
                    .context(error::GetTest)?],
                None => tests_api.get_all_tests().await.context(error::GetTest)?,
            };
            let mut all_finished = true;
            for test in tests {
                let test_result = TestResult::from_test(&test);
                if !test_result.is_finished() {
                    all_finished = false;
                }
                if test_result.failed() {
                    failures.push(test_result.name.clone())
                }
                status_results.add_test_result(test_result)
            }
            if !self.json {
                println!("{}", status_results);
            }
            if !self.wait || all_finished {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(2000)).await;
        }
        if self.json {
            println!(
                "{}",
                serde_json::to_string(&status_results).context(error::JsonSerialize)?
            )
        }
        if !failures.is_empty() {
            Err(error::Error::FailedTest { tests: failures })
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct StatusResults {
    tests: HashMap<String, TestResult>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TestResult {
    name: String,
    lifecycle: Option<Lifecycle>,
    run_state: Option<RunState>,
    passed: Option<u64>,
    failed: Option<u64>,
    skipped: Option<u64>,
}

impl StatusResults {
    fn new() -> Self {
        Self {
            tests: HashMap::new(),
        }
    }

    fn add_test_result(&mut self, test_result: TestResult) {
        self.tests.insert(test_result.name.clone(), test_result);
    }
}

impl Display for StatusResults {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (_name, result) in &self.tests {
            write!(f, "{}\n\n", result)?;
        }

        Ok(())
    }
}

impl TestResult {
    fn from_test(test: &Test) -> Self {
        let name = test.metadata.name.clone().unwrap_or("".to_string());
        let mut passed = None;
        let mut failed = None;
        let mut skipped = None;
        let mut lifecycle = None;
        let mut run_state = None;
        if let Some(status) = &test.status {
            if let Some(controller) = &status.controller {
                lifecycle = Some(controller.lifecycle);
            }
            if let Some(agent) = &status.agent {
                if let Some(results) = &agent.results {
                    passed = Some(results.num_passed);
                    failed = Some(results.num_failed);
                    skipped = Some(results.num_skipped);
                }
                run_state = Some(agent.run_state);
            }
        }

        Self {
            name,
            lifecycle,
            run_state,
            passed,
            failed,
            skipped,
        }
    }

    fn is_finished(&self) -> bool {
        // TODO update the finished condition.
        if let Some(lifecycle) = self.lifecycle {
            lifecycle == Lifecycle::TestPodExited
                || lifecycle == Lifecycle::TestPodError
                || lifecycle == Lifecycle::TestPodDone
                || lifecycle == Lifecycle::TestPodFailed
        } else {
            false
        }
    }

    fn failed(&self) -> bool {
        if let Some(run_state) = self.run_state {
            run_state == RunState::Error
        } else {
            false
        }
    }
}

impl Display for TestResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Test Name: {}\n", self.name)?;
        write!(
            f,
            "Controller State: {}\n",
            self.lifecycle.map_or("".to_string(), |l| l.to_string())
        )?;
        write!(
            f,
            "Agent State: {}\n",
            self.run_state.map_or("".to_string(), |r| r.to_string())
        )?;
        write!(
            f,
            "Passed: {}\n",
            self.passed.map_or("".to_string(), |x| x.to_string())
        )?;
        write!(
            f,
            "Failed: {}\n",
            self.failed.map_or("".to_string(), |x| x.to_string())
        )?;
        write!(
            f,
            "Skipped: {}\n",
            self.skipped.map_or("".to_string(), |x| x.to_string())
        )?;

        Ok(())
    }
}
