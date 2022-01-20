use crate::error::{self, Result};
use futures::{stream, StreamExt};
use k8s_openapi::api::core::v1::Pod;
use kube::api::ListParams;
use kube::core::object::HasStatus;
use kube::{Api, Client, ResourceExt};
use model::clients::{CrdClient, ResourceClient, TestClient};
use model::constants::{LABEL_COMPONENT, NAMESPACE};
use model::{Resource, TaskState, Test, TestUserState};
use serde::{Deserialize, Serialize};
use snafu::ResultExt;
use std::collections::HashMap;
use std::fmt::Display;
use structopt::StructOpt;

/// Check the status of a TestSys object.
#[derive(Debug, StructOpt)]
pub(crate) struct Status {
    /// Check the status of the `Test`s provided, or all `Test`s if no specific resource is provided.
    #[structopt(long = "tests", short = "t")]
    tests: Option<Vec<String>>,

    /// Check the status of the `Resource`s provided, or all `Resource`s if no specific resource is provided.
    #[structopt(long = "resources", short = "r")]
    resources: Option<Vec<String>>,

    /// Check the status of the testsys controller
    #[structopt(long, short = "c")]
    controller: bool,

    /// Continue checking the status of the test/resources(s) until all have completed.
    #[structopt(long = "wait")]
    wait: bool,

    /// Output the results in JSON format.
    #[structopt(long = "json")]
    json: bool,
}

impl Status {
    pub(crate) async fn run(&self, k8s_client: Client) -> Result<()> {
        let tests_api = TestClient::new_from_k8s_client(k8s_client.clone());
        let resources_api = ResourceClient::new_from_k8s_client(k8s_client.clone());
        let pod_api = Api::<Pod>::namespaced(k8s_client, NAMESPACE);
        let mut failures;
        let mut status_results;
        loop {
            failures = Vec::new();
            status_results = StatusResults::new();
            let mut all_finished = true;
            if self.controller {
                status_results.controller_is_running = Some(is_controller_running(&pod_api).await?);
            }
            let tests = self.tests(&tests_api).await?;
            let resources = self.resources(&resources_api).await?;
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
            for resource in resources {
                let resource_result = ResourceResult::from_resource(&resource);
                if !resource_result.is_finished() {
                    all_finished = false;
                }
                if resource_result.failed() {
                    failures.push(resource_result.name.clone())
                }
                status_results.add_resource_result(resource_result)
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
                serde_json::to_string(&status_results).context(error::JsonSerializeSnafu)?
            )
        }
        if !failures.is_empty() {
            Err(error::Error::FailedTest { tests: failures })
        } else {
            Ok(())
        }
    }

    async fn tests(&self, test_client: &TestClient) -> Result<Vec<Test>> {
        let test_names = match &self.tests {
            Some(tests) => tests,
            None => return Ok(Vec::new()),
        };
        if test_names.is_empty() {
            test_client
                .get_all()
                .await
                .context(error::GetSnafu { what: "all_tests" })
        } else {
            stream::iter(test_names)
                .then(|test_name| async move {
                    test_client.get(&test_name).await.context(error::GetSnafu {
                        what: test_name.clone(),
                    })
                })
                .collect::<Vec<Result<Test>>>()
                .await
                .into_iter()
                .collect::<Result<Vec<Test>>>()
        }
    }

    async fn resources(&self, resource_client: &ResourceClient) -> Result<Vec<Resource>> {
        let resource_names = match &self.resources {
            Some(resources) => resources,
            None => return Ok(Vec::new()),
        };
        if resource_names.is_empty() {
            resource_client.get_all().await.context(error::GetSnafu {
                what: "all_resources",
            })
        } else {
            stream::iter(resource_names)
                .then(|resource_name| async move {
                    resource_client
                        .get(&resource_name)
                        .await
                        .context(error::GetSnafu {
                            what: resource_name.clone(),
                        })
                })
                .collect::<Vec<Result<Resource>>>()
                .await
                .into_iter()
                .collect::<Result<Vec<Resource>>>()
        }
    }
}

async fn is_controller_running(pod_api: &Api<Pod>) -> Result<bool> {
    let pods = pod_api
        .list(&ListParams {
            label_selector: Some(format!("{}={}", LABEL_COMPONENT, "controller")),
            ..Default::default()
        })
        .await
        .context(error::GetPodSnafu {
            test_name: "controller",
        })?
        .items;
    if pods.is_empty() {
        return Ok(false);
    }
    for pod in pods {
        if !pod
            .status
            .as_ref()
            .and_then(|s| s.phase.as_ref().map(|s| s == "Running"))
            .unwrap_or(false)
        {
            return Ok(false);
        }
    }

    Ok(true)
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct StatusResults {
    tests: HashMap<String, TestResult>,
    resources: HashMap<String, ResourceResult>,
    controller_is_running: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TestResult {
    name: String,
    state: TestUserState,
    passed: Option<u64>,
    failed: Option<u64>,
    skipped: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ResourceResult {
    name: String,
    create_state: TaskState,
    delete_state: TaskState,
}

impl StatusResults {
    fn new() -> Self {
        Self {
            tests: HashMap::new(),
            resources: HashMap::new(),
            controller_is_running: None,
        }
    }

    fn add_test_result(&mut self, test_result: TestResult) {
        self.tests.insert(test_result.name.clone(), test_result);
    }

    fn add_resource_result(&mut self, resource_result: ResourceResult) {
        self.resources
            .insert(resource_result.name.clone(), resource_result);
    }
}

impl Display for StatusResults {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for result in self.tests.values() {
            write!(f, "{}\n\n", result)?;
        }

        for result in self.resources.values() {
            write!(f, "{}\n\n", result)?;
        }

        if let Some(running) = self.controller_is_running {
            write!(f, "Controller Running: {}", running)?;
        }

        Ok(())
    }
}

impl TestResult {
    fn from_test(test: &Test) -> Self {
        let name = test.metadata.name.clone().unwrap_or_else(|| "".to_string());
        let mut passed = None;
        let mut failed = None;
        let mut skipped = None;
        let test_user_state = test.test_user_state();
        if let Some(results) = &test.agent_status().results {
            passed = Some(results.num_passed);
            failed = Some(results.num_failed);
            skipped = Some(results.num_skipped);
        }

        Self {
            name,
            state: test_user_state,
            passed,
            failed,
            skipped,
        }
    }

    fn is_finished(&self) -> bool {
        match self.state {
            TestUserState::Unknown | TestUserState::Starting | TestUserState::Running => false,
            TestUserState::NoTests
            | TestUserState::Passed
            | TestUserState::Failed
            | TestUserState::Error
            | TestUserState::ResourceError
            | TestUserState::Deleting => true,
        }
    }

    fn failed(&self) -> bool {
        match self.state {
            TestUserState::Unknown
            | TestUserState::Starting
            | TestUserState::Running
            | TestUserState::NoTests
            | TestUserState::Passed => false,
            TestUserState::Failed | TestUserState::Error | TestUserState::ResourceError => true,
            TestUserState::Deleting => false,
        }
    }
}

impl Display for TestResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Test Name: {}", self.name)?;
        writeln!(f, "Test State: {}", self.state)?;
        writeln!(
            f,
            "Passed: {}",
            self.passed.map_or("".to_string(), |x| x.to_string())
        )?;
        writeln!(
            f,
            "Failed: {}",
            self.failed.map_or("".to_string(), |x| x.to_string())
        )?;
        writeln!(
            f,
            "Skipped: {}",
            self.skipped.map_or("".to_string(), |x| x.to_string())
        )?;

        Ok(())
    }
}

impl ResourceResult {
    fn from_resource(resource: &Resource) -> Self {
        let name = resource.name();
        let mut create_state = TaskState::Unknown;
        let mut delete_state = TaskState::Unknown;
        if let Some(status) = resource.status() {
            create_state = status.creation.task_state;
            delete_state = status.destruction.task_state;
        }

        Self {
            name,
            create_state,
            delete_state,
        }
    }

    fn is_finished(&self) -> bool {
        self.create_state == TaskState::Completed || self.create_state == TaskState::Error
    }

    fn failed(&self) -> bool {
        self.create_state == TaskState::Error
    }
}

impl Display for ResourceResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Resource Name: {}", self.name)?;
        writeln!(f, "Resource Creation State: {:?}", self.create_state)?;
        writeln!(f, "Resource Deletion State: {:?}", self.delete_state)?;
        Ok(())
    }
}
