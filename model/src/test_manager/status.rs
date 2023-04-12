use crate::{Crd, TaskState};
use kube::{core::object::HasStatus, ResourceExt};
use serde::Serialize;
use std::cmp::max;
use std::fmt::Display;
use tabled::builder::Builder;
use tabled::locator::ByColumnName;
use tabled::object::Rows;
use tabled::width::MinWidth;
use tabled::{Alignment, Disable, Modify, Style, Table, Width};

#[derive(Clone)]
pub struct StatusColumn {
    header: String,
    //  If the Vec contains more than 1 value, each value will occupy a single box stacked
    //  vertically. If no value should be printed an empty Vec should be returned.
    values: fn(&Crd) -> Vec<String>,
    alignment: TextAlignment,
    width: Option<usize>,
}

impl std::fmt::Debug for StatusColumn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdditionalColumn")
            .field("header", &self.header)
            .field("alignment", &self.alignment)
            .field("width", &self.width)
            .finish()
    }
}

impl Default for StatusColumn {
    fn default() -> Self {
        Self {
            values: |_| Default::default(),
            header: Default::default(),
            alignment: Default::default(),
            width: Default::default(),
        }
    }
}

#[derive(Default, Clone, Debug)]
pub enum TextAlignment {
    #[default]
    Left,
    Right,
}

impl TextAlignment {
    fn horizontal(&self) -> Alignment {
        match self {
            TextAlignment::Left => Alignment::left(),
            TextAlignment::Right => Alignment::right(),
        }
    }
}

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
    crds: Vec<Crd>,
    #[serde(skip)]
    columns: Vec<StatusColumn>,
}

impl StatusSnapshot {
    pub(super) fn new(crds: Vec<Crd>) -> Self {
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
            crds,
            columns: Default::default(),
        }
    }

    pub fn new_column<S1>(&mut self, header: S1, f: fn(&Crd) -> Vec<String>) -> &mut Self
    where
        S1: Into<String>,
    {
        self.columns.push(StatusColumn {
            header: header.into(),
            values: f,
            ..Default::default()
        });
        self
    }

    pub fn add_column(&mut self, col: StatusColumn) -> &mut Self {
        self.columns.push(col);
        self
    }

    pub fn set_columns(&mut self, columns: Vec<StatusColumn>) -> &mut Self {
        self.columns = columns;
        self
    }
}

impl From<&StatusSnapshot> for Table {
    fn from(snapshot: &StatusSnapshot) -> Self {
        let headers: Vec<_> = snapshot
            .columns
            .iter()
            .map(|column| vec![column.header.to_string()])
            .collect();
        let status_data = snapshot
            .crds
            .iter()
            .map(|crd| snapshot.columns.iter().map(|column| (column.values)(crd)))
            .fold(headers, |data, x| {
                let mut row_count = 0;
                // Determine how many rows this CRD will take in the status table.
                for col in x.clone() {
                    row_count = max(row_count, col.len());
                }
                data.into_iter()
                    .zip(x)
                    .map(|(mut data_col, mut crd_data)| {
                        // Extend each Vec from this CRD to have the same number of rows.
                        crd_data.resize(row_count, "".into());
                        data_col.extend(crd_data);
                        data_col
                    })
                    .collect()
            });

        let mut table = Builder::from_iter(status_data)
            .index()
            // index is needed to use `transpose` however, we don't want the index to show, so
            // `hide_index` is used as well.
            .hide_index()
            .transpose()
            .to_owned()
            .build();

        table
            .with(Style::blank())
            // Remove the headers that `tabled` adds.
            .with(Disable::row(Rows::first()));

        // Apply the custom formatting for each column
        for column in &snapshot.columns {
            if let Some(width) = column.width {
                table.with(
                    Modify::new(ByColumnName::new(&column.header))
                        .with(column.alignment.horizontal())
                        .with(Width::truncate(width)),
                );
            } else {
                table.with(
                    Modify::new(ByColumnName::new(&column.header))
                        .with(column.alignment.horizontal()),
                );
            }
        }

        table
    }
}

impl Display for StatusSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut table: Table = self.into();
        if let Some(width) = f.width() {
            // If we received a width, we use it
            write!(
                f,
                "{}",
                table
                    .with(Width::truncate(width))
                    .with(MinWidth::new(width))
            )
        } else {
            // Otherwise we do nothing special
            write!(f, "{}", table)
        }
    }
}

// The following contains several common status columns for users.
impl StatusColumn {
    pub fn name() -> StatusColumn {
        StatusColumn {
            header: "NAME".to_string(),
            values: |crd| crd.name().into_iter().collect(),
            ..Default::default()
        }
    }

    pub fn crd_type() -> StatusColumn {
        StatusColumn {
            header: "TYPE".to_string(),
            values: crd_type,
            ..Default::default()
        }
    }

    pub fn state() -> StatusColumn {
        StatusColumn {
            header: "STATE".to_string(),
            values: crd_state,
            ..Default::default()
        }
    }

    pub fn passed() -> StatusColumn {
        StatusColumn {
            header: "PASSED".to_string(),
            values: |crd| crd_results(crd, ResultType::Passed),
            alignment: TextAlignment::Right,
            width: Some(6),
        }
    }

    pub fn failed() -> StatusColumn {
        StatusColumn {
            header: "FAILED".to_string(),
            values: |crd| crd_results(crd, ResultType::Failed),
            alignment: TextAlignment::Right,
            width: Some(6),
        }
    }

    pub fn skipped() -> StatusColumn {
        StatusColumn {
            header: "SKIPPED".to_string(),
            values: |crd| crd_results(crd, ResultType::Skipped),
            alignment: TextAlignment::Right,
            width: Some(7),
        }
    }

    pub fn last_update() -> StatusColumn {
        StatusColumn {
            header: "LAST UPDATE".to_string(),
            values: crd_time,
            alignment: TextAlignment::Left,
            width: Some(20),
        }
    }

    pub fn progress() -> StatusColumn {
        StatusColumn {
            header: "PROGRESS".to_string(),
            values: crd_progress,
            ..Default::default()
        }
    }
}

/// Determine the time of the last update to the CRD
fn crd_time(crd: &Crd) -> Vec<String> {
    match crd {
        Crd::Test(test) => test
            .status
            .as_ref()
            .and_then(|status| status.last_update.to_owned()),
        Crd::Resource(resource) => resource
            .status()
            .and_then(|status| status.last_update.to_owned()),
    }
    .into_iter()
    .collect()
}

/// Determine the type of the CRD
fn crd_type(crd: &Crd) -> Vec<String> {
    match crd {
        Crd::Test(_) => vec!["Test".to_string()],
        Crd::Resource(_) => vec!["Resource".to_string()],
    }
}

/// Determine the state of the CRD
fn crd_state(crd: &Crd) -> Vec<String> {
    match crd {
        Crd::Test(test) => vec![test.test_user_state().to_string()],
        Crd::Resource(resource) => {
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
            vec![state.to_string()]
        }
    }
}

enum ResultType {
    Passed,
    Failed,
    Skipped,
}

/// Collect the
fn crd_results(crd: &Crd, res_type: ResultType) -> Vec<String> {
    match crd {
        Crd::Resource(_) => Default::default(),
        Crd::Test(test) => {
            let mut results = Vec::new();
            let test_results = &test.agent_status().results;
            let current_test = &test.agent_status().current_test;
            let test_iter = test_results.iter().peekable();
            for result in test_iter.chain(current_test) {
                results.push(
                    match res_type {
                        ResultType::Passed => result.num_passed,
                        ResultType::Failed => result.num_failed,
                        ResultType::Skipped => result.num_skipped,
                    }
                    .to_string(),
                );
            }
            results
        }
    }
}

fn crd_progress(crd: &Crd) -> Vec<String> {
    match crd {
        Crd::Resource(_) => Default::default(),
        Crd::Test(test) => test
            .agent_status()
            .current_test
            .as_ref()
            .and_then(|res| res.other_info.to_owned())
            .into_iter()
            .collect(),
    }
}
