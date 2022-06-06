use crate::error::{self, Result};
use futures::{stream, StreamExt};
use k8s_openapi::api::core::v1::Pod;
use kube::api::ListParams;
use kube::core::object::HasStatus;
use kube::{Api, Client, ResourceExt};
use model::clients::{CrdClient, ResourceClient, TestClient};
use model::constants::{LABEL_COMPONENT, NAMESPACE};
use model::{Resource, TaskState, Test};
use serde::Serialize;
use snafu::ResultExt;
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
        let mut status_results = Results::default();
        if self.controller {
            status_results.add_controller(is_controller_running(&pod_api).await?);
        }
        let tests = self.tests(&tests_api).await?;
        let resources = self.resources(&resources_api).await?;
        for test in tests {
            status_results.add_test(&test)
        }
        for resource in resources {
            status_results.add_resource(&resource)
        }

        if !self.json {
            let (width, _) = terminal_size().ok().unwrap_or((120, 0));
            status_results.width = width;
            status_results.draw();
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

#[derive(Serialize)]
struct Results {
    #[serde(skip_serializing)]
    width: u16,
    results: Vec<ResultRow>,
    #[serde(skip_serializing)]
    min_name_width: u16,
    #[serde(skip_serializing)]
    min_object_width: u16,
    #[serde(skip_serializing)]
    min_state_width: u16,
    #[serde(skip_serializing)]
    min_passed_width: u16,
    #[serde(skip_serializing)]
    min_skipped_width: u16,
    #[serde(skip_serializing)]
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
    /// Adds a new `ResultRow` for each `TestResults` in test, or a single row if no `TestsResults` are available.
    fn add_test(&mut self, test: &Test) {
        let name = test.metadata.name.clone().unwrap_or_else(|| "".to_string());
        let state = test.test_user_state().to_string();
        let results = &test.agent_status().results;
        if results.is_empty() {
            self.results.push(ResultRow {
                name,
                object_type: "Test".to_string(),
                state,
                passed: None,
                skipped: None,
                failed: None,
            })
        } else {
            for (test_count, result) in results.iter().enumerate() {
                let retry_name = if test_count == 0 {
                    name.clone()
                } else {
                    format!("{}-retry-{}", name, test_count)
                };
                self.results.push(ResultRow {
                    name: retry_name,
                    object_type: "Test".to_string(),
                    state: state.clone(),
                    passed: Some(result.num_passed),
                    skipped: Some(result.num_skipped),
                    failed: Some(result.num_failed),
                });
            }
        }
    }

    fn add_resource(&mut self, resource: &Resource) {
        let name = resource.name();
        let mut create_state = TaskState::Unknown;
        let mut delete_state = TaskState::Unknown;
        if let Some(status) = resource.status() {
            create_state = status.creation.task_state;
            delete_state = status.destruction.task_state;
        }
        let state = match delete_state {
            TaskState::Unknown => create_state,
            _ => delete_state,
        };

        self.results.push(ResultRow {
            name,
            object_type: "Resource".to_string(),
            state: state.to_string(),
            passed: None,
            skipped: None,
            failed: None,
        });
    }

    fn add_controller(&mut self, running: bool) {
        self.results.push(ResultRow {
            name: "Controller".to_string(),
            object_type: "Controller".to_string(),
            state: if running { "Running" } else { "" }.to_string(),
            passed: None,
            skipped: None,
            failed: None,
        });
    }

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

#[derive(Tabled, Default, Clone, Serialize)]
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
