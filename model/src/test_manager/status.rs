use super::StatusProgress;
use crate::{Crd, TaskState};
use k8s_openapi::api::core::v1::PodStatus;
use kube::{core::object::HasStatus, ResourceExt};
use serde::Serialize;
use tabled::object::Segment;
use tabled::{
    Alignment, Concat, Extract, Header, MaxWidth, MinWidth, Modify, Style, Table, TableIteratorExt,
    Tabled,
};

/// `StatusSnapshot` represents the status of a set of testsys objects (including the controller).
/// `StatusSnapshot::to_string()` is used to create a table representation of the status.
/// `StatusSnapshot` can also be used with `json::to_string()` to create a json representation of
/// the testsys objects.
/// To add a new column to the status table, `new_column` can be used.
/// `status.new_column("extra column", |crd| crd.name());`
#[derive(Debug, Serialize)]
pub struct StatusSnapshot {
    finished: bool,
    passed: bool,
    failed_tests: Vec<String>,
    controller_status: Option<PodStatus>,
    crds: Vec<Crd>,
    #[serde(skip)]
    additional_columns: Vec<AdditionalColumn>,
    #[serde(skip)]
    with_progress: Option<StatusProgress>,
    #[serde(skip)]
    with_time: bool,
}

impl StatusSnapshot {
    pub(super) fn new(controller_status: Option<PodStatus>, crds: Vec<Crd>) -> Self {
        let mut passed = true;
        let mut finished = true;
        let mut failed_tests = Vec::new();
        for crd in &crds {
            match crd {
                Crd::Test(test) => match test.agent_status().task_state {
                    TaskState::Unknown | TaskState::Running => {
                        passed = false;
                        finished = false
                    }
                    TaskState::Error => {
                        passed = false;
                        failed_tests.push(test.name_any());
                    }
                    _ => continue,
                },
                Crd::Resource(resource) => {
                    match resource.creation_task_state() {
                        TaskState::Unknown | TaskState::Running => {
                            passed = false;
                            finished = false
                        }
                        TaskState::Error => passed = false,
                        _ => continue,
                    };
                    match resource.destruction_task_state() {
                        TaskState::Unknown | TaskState::Running => {
                            // Indicate that some pods still may be running.
                            finished = false
                        }
                        _ => continue,
                    }
                }
            }
        }
        Self {
            passed,
            finished,
            failed_tests,
            controller_status,
            crds,
            additional_columns: Default::default(),
            with_progress: None,
            with_time: false,
        }
    }

    pub fn new_column<S1>(&mut self, header: S1, f: fn(&Crd) -> Option<String>) -> &mut Self
    where
        S1: Into<String>,
    {
        self.additional_columns.push(AdditionalColumn {
            header: header.into(),
            value: f,
        });
        self
    }

    pub fn with_progress(&mut self, status_progress: StatusProgress) -> &mut Self {
        self.with_progress = Some(status_progress);
        self
    }

    pub fn with_time(&mut self) -> &mut Self {
        self.with_time = true;
        self
    }

    fn progress_column(&self) -> Option<Table> {
        let mut crds = self.crds.clone();
        crds.sort_by_key(|crd| crd.name());
        self.with_progress.as_ref().map(|with_progress| {
            // Convert the CRDs to an iterator
            crds.iter()
                // For each CRD create a `Vec` containing the status for that CRD
                // It needs to be a `Vec` because each `TestResults` is displayed in it's own
                // row. `flat_map` will automatically flatten the `Iterator<Vec>` to
                // `Iterator<Option<String>>`
                .flat_map(|crd| match crd {
                    Crd::Test(test) => {
                        if !test.agent_status().results.is_empty() {
                            test.agent_status()
                                .results
                                .iter()
                                // For each `TestResults`, if the test progress should be included, add
                                // it to the `Vec`, if not just add `None`
                                .map(|result| {
                                    if matches!(with_progress, StatusProgress::WithTests) {
                                        result.other_info.to_owned()
                                    } else {
                                        None
                                    }
                                })
                                .collect()
                        } else {
                            // If there are no test results, a line will still be there
                            vec![Some("No test results".to_string())]
                        }
                    }
                    // Get the status of each resource and wrap it in a `Vec` to match types
                    // with the `Test` branch.
                    Crd::Resource(resource) => vec![resource.status().and_then(|status| {
                        status.agent_info.as_ref().and_then(|agent_info| {
                            agent_info
                                .get("currentStatus")
                                .and_then(|info| info.as_str().map(|info| info.to_string()))
                        })
                    })],
                })
                // Convert the `Option<String>` to `String`
                .map(Option::unwrap_or_default)
                .table()
                .with(MaxWidth::wrapping(50))
                .with(Extract::segment(1.., 0..))
                .with(Header("STATUS"))
        })
    }

    fn time_column(&self) -> Option<Table> {
        let mut crds = self.crds.clone();
        crds.sort_by_key(|crd| crd.name());
        self.with_time.then(|| {
            // Convert the CRDs to an iterator
            crds.iter()
                // For each CRD create a `Vec` containing the status for that CRD
                // It needs to be a `Vec` because each `TestResults` is displayed in it's own
                // row. `flat_map` will automatically flatten the `Iterator<Vec>` to
                // `Iterator<Option<String>>`
                .flat_map(|crd| vec![crd_time(crd); crd_rows(crd)])
                // Convert the `Option<String>` to `String`
                .map(Option::unwrap_or_default)
                .table()
                .with(MinWidth::new(20))
                .with(Extract::segment(1.., 0..))
                .with(Header("LAST UPDATE"))
        })
    }
}

impl std::fmt::Display for StatusSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let table: Table = self.into();
        if let Some(width) = f.width() {
            // If we received a width, we use it
            write!(
                f,
                "{}",
                table
                    .with(MaxWidth::truncating(width))
                    .with(MinWidth::new(width))
            )
        } else {
            // Otherwise we do nothing special
            write!(f, "{}", table)
        }
    }
}

impl From<&StatusSnapshot> for Table {
    fn from(status: &StatusSnapshot) -> Self {
        let mut crds = status.crds.clone();
        crds.sort_by_key(|crd| crd.name());
        let mut results = Vec::new();
        if let Some(controller_status) = &status.controller_status {
            results.push(ResultRow {
                name: "controller".to_string(),
                object_type: "Controller".to_string(),
                state: controller_status
                    .phase
                    .clone()
                    .unwrap_or_else(|| "".to_string()),
                passed: None,
                skipped: None,
                failed: None,
            });
        }
        for crd in &crds {
            results.extend::<Vec<ResultRow>>(crd.into());
        }

        let progress_column = status.progress_column();

        let time_column = status.time_column();

        // An extra line for the controller if it's status is being reported.
        let controller_line = if status.controller_status.is_some() {
            Some("".to_string())
        } else {
            None
        };

        progress_column
            .into_iter()
            .chain(time_column.into_iter())
            .chain(
                status
                    .additional_columns
                    .iter()
                    // Create a table for each additional column so they can all be merged into a single table.
                    .map(|additional_column| {
                        // Add the requested header and a blank string for the controller line in the status table.
                        vec![additional_column.header.clone()]
                            .into_iter()
                            .chain(controller_line.clone())
                            // Add a row for each crd based on the function provided.
                            .chain(status.crds.iter().flat_map(|crd| {
                                vec![
                                    (additional_column.value)(crd).unwrap_or_default();
                                    crd_rows(crd)
                                ]
                            }))
                            // Convert the data for this column into a table.
                            .table()
                            .with(Extract::segment(1.., 0..))
                    }),
            )
            // Add each additional column to the standard results table (`Table::new(results)`).
            .fold(Table::new(results), |table1, table2| {
                table1.with(Concat::horizontal(table2))
            })
            .with(Style::blank())
            .with(Modify::new(Segment::all()).with(Alignment::left()))
    }
}

#[derive(Tabled, Default, Clone, Serialize)]
struct ResultRow {
    #[tabled(rename = "NAME")]
    name: String,
    #[tabled(rename = "TYPE")]
    object_type: String,
    #[tabled(rename = "STATE")]
    state: String,
    #[tabled(rename = "PASSED")]
    #[tabled(display_with = "display_option")]
    passed: Option<u64>,
    #[tabled(rename = "SKIPPED")]
    #[tabled(display_with = "display_option")]
    skipped: Option<u64>,
    #[tabled(rename = "FAILED")]
    #[tabled(display_with = "display_option")]
    failed: Option<u64>,
}

fn display_option(o: &Option<u64>) -> String {
    match o {
        Some(count) => format!("{}", count),
        None => "".to_string(),
    }
}

impl From<&Crd> for Vec<ResultRow> {
    fn from(crd: &Crd) -> Self {
        let mut results = Vec::new();
        match crd {
            Crd::Test(test) => {
                let name = test.metadata.name.clone().unwrap_or_else(|| "".to_string());
                let state = test.test_user_state().to_string();
                let test_results = &test.agent_status().results;
                if test_results.is_empty() {
                    results.push(ResultRow {
                        name,
                        object_type: "Test".to_string(),
                        state,
                        passed: None,
                        skipped: None,
                        failed: None,
                    })
                } else {
                    for (test_count, result) in test_results.iter().enumerate() {
                        let retry_name = if test_count == 0 {
                            name.clone()
                        } else {
                            format!("{}-retry-{}", name, test_count)
                        };
                        results.push(ResultRow {
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
            Crd::Resource(resource) => {
                let name = resource.name_any();
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

                results.push(ResultRow {
                    name,
                    object_type: "Resource".to_string(),
                    state: state.to_string(),
                    passed: None,
                    skipped: None,
                    failed: None,
                });
            }
        };
        results
    }
}

struct AdditionalColumn {
    header: String,
    value: fn(&Crd) -> Option<String>,
}

impl std::fmt::Debug for AdditionalColumn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdditionalColumn")
            .field("header", &self.header)
            .finish()
    }
}

/// Determine the number of status rows that will be occupied by this CRD
fn crd_rows(crd: &Crd) -> usize {
    match crd {
        Crd::Test(test) => {
            let retry_count = test.agent_status().results.len();
            if retry_count != 0 {
                retry_count
            } else {
                1
            }
        }
        Crd::Resource(_) => 1,
    }
}

/// Determine the time of the last update to the CRD
fn crd_time(crd: &Crd) -> Option<String> {
    match crd {
        Crd::Test(test) => test
            .status
            .as_ref()
            .and_then(|status| status.last_update.to_owned()),
        Crd::Resource(resource) => resource
            .status()
            .and_then(|status| status.last_update.to_owned()),
    }
}
