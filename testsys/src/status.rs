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
use tabled::{Alignment, Column, Full, MaxWidth, Modify, Style, Table, Tabled};
use termion::terminal_size;

/// Check the status of a TestSys object.
#[derive(Debug, StructOpt)]
pub(crate) struct Status {
    /// Check the status of the `Resource`s provided, or all `Resource`s if no specific resource is provided.
    #[structopt(long = "resources", short = "r")]
    resources: Option<Vec<String>>,

    /// Check the status of the testsys controller
    #[structopt(long, short = "c")]
    controller: bool,

    /// Output the results in JSON format.
    #[structopt(long = "json")]
    json: bool,
}

impl Status {
    pub(crate) async fn run(&self, k8s_client: Client) -> Result<()> {
        let tests_api = TestClient::new_from_k8s_client(k8s_client.clone());
        let resources_api = ResourceClient::new_from_k8s_client(k8s_client.clone());
        let pod_api = Api::<Pod>::namespaced(k8s_client, NAMESPACE);
        let mut status_results;
        status_results = StatusResults::new();
        if self.controller {
            status_results.controller_is_running = Some(is_controller_running(&pod_api).await?);
        }
        let tests = self.tests(&tests_api).await?;
        let resources = self.resources(&resources_api).await?;
        for test in tests {
            let test_result = TestResult::from_test(&test);
            status_results.add_test_result(test_result)
        }
        for resource in resources {
            let resource_result = ResourceResult::from_resource(&resource);
            status_results.add_resource_result(resource_result)
        }

        if !self.json {
            let (width, _) = terminal_size().ok().unwrap_or((120, 0));
            status_results.draw(width);
        } else {
            println!(
                "{}",
                serde_json::to_string(&status_results).context(error::JsonSerializeSnafu)?
            )
        }

        Ok(())
    }

    async fn tests(&self, test_client: &TestClient) -> Result<Vec<Test>> {
        test_client
            .get_all()
            .await
            .context(error::GetSnafu { what: "all_tests" })
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

    fn draw(&self, width: u16) {
        let mut results = Vec::new();
        if let Some(controller_running) = self.controller_is_running {
            results.push(ResultRow {
                name: "Controller".to_string(),
                object_type: "Controller".to_string(),
                state: if controller_running { "Running" } else { "" }.to_string(),
                ..Default::default()
            })
        }
        for resource_result in self.resources.values() {
            results.push(resource_result.into());
        }
        for test_result in self.tests.values() {
            results.push(test_result.into());
        }
        let results_table = Results {
            width,
            results,
            ..Default::default()
        };

        results_table.draw();
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

impl From<&TestResult> for ResultRow {
    fn from(test_result: &TestResult) -> ResultRow {
        ResultRow {
            name: test_result.name.clone(),
            object_type: "Test".to_string(),
            state: test_result.state.to_string(),
            passed: test_result.passed,
            skipped: test_result.skipped,
            failed: test_result.failed,
        }
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
}

impl Display for ResourceResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Resource Name: {}", self.name)?;
        writeln!(f, "Resource Creation State: {:?}", self.create_state)?;
        writeln!(f, "Resource Deletion State: {:?}", self.delete_state)?;
        Ok(())
    }
}

impl From<&ResourceResult> for ResultRow {
    fn from(resource_result: &ResourceResult) -> ResultRow {
        let state = match resource_result.delete_state {
            TaskState::Unknown => resource_result.create_state,
            _ => resource_result.delete_state,
        };
        ResultRow {
            name: resource_result.name.clone(),
            object_type: "Resource".to_string(),
            state: state.to_string(),
            passed: None,
            skipped: None,
            failed: None,
        }
    }
}

struct Results {
    width: u16,
    results: Vec<ResultRow>,
    min_name_width: u16,
    min_object_width: u16,
    min_state_width: u16,
    min_passed_width: u16,
    min_skipped_width: u16,
    min_failed_width: u16,
}

impl Default for Results {
    fn default() -> Self {
        Self {
            width: 100,
            results: Vec::new(),
            min_name_width: 4,
            min_object_width: 4,
            min_state_width: 5,
            min_passed_width: 6,
            min_skipped_width: 7,
            min_failed_width: 6,
        }
    }
}

impl Results {
    fn draw(&self) {
        if self.width
            < self.min_name_width
                + self.min_object_width
                + self.min_state_width
                + self.min_passed_width
                + self.min_skipped_width
                + self.min_failed_width
                + 25
        {
            println!("The screen is not wide enough.");
            return;
        }
        let width_diff = self.width
            - self.min_name_width
            - self.min_object_width
            - self.min_state_width
            - self.min_passed_width
            - self.min_skipped_width
            - self.min_failed_width
            - 25;
        let mut name = self.min_name_width;
        let mut object = self.min_object_width;
        let mut state = self.min_state_width;
        let passed = self.min_passed_width;
        let skipped = self.min_skipped_width;
        let failed = self.min_failed_width;
        if width_diff < 18 {
            let diff = width_diff / 3;
            state += diff;
            object += diff;
            name += width_diff - 2 * diff;
        } else {
            name += width_diff - 12;
            object += 6;
            state += 6;
        }

        let mut sorted_results = self.results.clone();
        sorted_results.sort_by(|a, b| a.name.cmp(&b.name));

        let table = Table::new(sorted_results)
            .with(Style::NO_BORDER)
            .with(Modify::new(Full).with(Alignment::left()))
            .with(Modify::new(Column(..1)).with(MaxWidth::truncating(name.into(), "..")))
            .with(Modify::new(Column(1..2)).with(MaxWidth::truncating(object.into(), "..")))
            .with(Modify::new(Column(2..3)).with(MaxWidth::truncating(state.into(), "..")))
            .with(Modify::new(Column(3..4)).with(MaxWidth::truncating(passed.into(), "")))
            .with(Modify::new(Column(4..5)).with(MaxWidth::truncating(skipped.into(), "")))
            .with(Modify::new(Column(6..)).with(MaxWidth::truncating(failed.into(), "")));
        print!("{}", table);
    }
}

#[derive(Tabled, Default, Clone)]
struct ResultRow {
    #[header("NAME")]
    name: String,
    #[header("TYPE")]
    object_type: String,
    #[header("STATE")]
    state: String,
    #[header("PASSED")]
    #[field(display_with = "display_option")]
    passed: Option<u64>,
    #[header("SKIPPED")]
    #[field(display_with = "display_option")]
    skipped: Option<u64>,
    #[header("FAILED")]
    #[field(display_with = "display_option")]
    failed: Option<u64>,
}

fn display_option(o: &Option<u64>) -> String {
    match o {
        Some(count) => format!("{}", count),
        None => "".to_string(),
    }
}
