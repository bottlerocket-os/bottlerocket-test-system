use crate::{Crd, TaskState};
use kube::{core::object::HasStatus, ResourceExt};
use serde::Serialize;
use std::cmp::max;
use std::fmt::Display;
use tabled::builder::Builder;
use tabled::settings::{
    location::ByColumnName,
    object::Rows,
    width::{MinWidth, Width},
    Alignment, Disable, Modify, Style,
};
use tabled::Table;

impl std::fmt::Debug for StatusColumn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdditionalColumn")
            .field("header", &self.header)
            .field("alignment", &self.alignment)
            .field("width", &self.width)
            .finish()
    }
}

type CondensedValuesFn = fn(&[Crd]) -> Vec<String>;

#[derive(Clone)]
pub struct StatusColumn {
    header: String,
    values: fn(&Crd) -> Vec<String>,
    condensed_values: Option<CondensedValuesFn>,
    alignment: TextAlignment,
    width: Option<usize>,
}

impl Default for StatusColumn {
    fn default() -> Self {
        Self {
            values: |_| Default::default(),
            condensed_values: None,
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
                        TaskState::Unknown | TaskState::Running => finished = false,
                        // Indicate that some pods still may be running.
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

    pub fn use_crds(&self) -> &Vec<Crd> {
        &self.crds
    }

    // Extracts data from one CRD
    fn extract_crd_data(crd: &Crd) -> Vec<Vec<String>> {
        vec![
            crd_type(crd),
            crd.labels()
                .get("testsys/type")
                .cloned()
                .into_iter()
                .collect(),
            crd.labels()
                .get("testsys/cluster")
                .cloned()
                .into_iter()
                .collect(),
            crd.labels()
                .get("testsys/arch")
                .cloned()
                .into_iter()
                .collect(),
            crd.labels()
                .get("testsys/variant")
                .cloned()
                .into_iter()
                .collect(),
            crd_state(crd),
            crd_results(crd, ResultType::Passed),
            crd_results(crd, ResultType::Failed),
            crd_results(crd, ResultType::Skipped),
        ]
    }

    pub fn extract_full_crd_data(&self) -> Vec<Vec<String>> {
        self.crds
            .iter()
            .map(|crd| {
                Self::extract_crd_data(crd)
                    .into_iter()
                    .flat_map(|v| {
                        if v.is_empty() {
                            vec![String::from("")]
                        } else {
                            v
                        }
                    })
                    .collect()
            })
            .collect()
    }

    // Uses the full CRD dataset and condenses the output if a 5-stage migration test fully passes
    pub fn condense_crd_data(&self) -> Vec<Vec<String>> {
        let full_crd_data = self.extract_full_crd_data();
        let mut result_data: Vec<Vec<String>> = vec![vec![]; 9];
        for i in 0..full_crd_data.len() {
            let curr_arch = &full_crd_data[i][3];
            let curr_variant = &full_crd_data[i][4];
            let curr_cluster = &full_crd_data[i][2];
            if full_crd_data[i][1] != "migration" {
                Self::add_row(&mut result_data, &full_crd_data, i);
            } else if i + 4 < full_crd_data.len()
                && full_crd_data[i + 4][1] == "migration"
                && (*curr_arch == full_crd_data[i + 4][3])
                && (*curr_variant == full_crd_data[i + 4][4])
                && (*curr_cluster == full_crd_data[i + 4][2])
            {
                if !full_crd_data[i + 4][6].is_empty()
                    && full_crd_data[i + 4][6].parse::<i32>().unwrap_or(0) > 0
                    && full_crd_data[i + 4][7].parse::<i32>().unwrap_or(0) == 0
                {
                    let sum_passed: String = (0..=4)
                        .map(|j| full_crd_data[i + j][6].parse::<i32>().unwrap_or(0))
                        .sum::<i32>()
                        .to_string();
                    let sum_skipped: String = (0..=4)
                        .map(|j| full_crd_data[i + j][8].parse::<i32>().unwrap_or(0))
                        .sum::<i32>()
                        .to_string();
                    Self::add_row_with_sums(
                        &mut result_data,
                        &full_crd_data,
                        i,
                        sum_passed,
                        sum_skipped,
                    );
                } else {
                    for j in 0..=4 {
                        Self::add_row(&mut result_data, &full_crd_data, i + j);
                    }
                }
            }
        }
        result_data
    }

    fn add_row(result_data: &mut [Vec<String>], full_crd_data: &[Vec<String>], i: usize) {
        if full_crd_data[i][0] == "Resource" {
            for (j, item) in result_data.iter_mut().enumerate().take(6) {
                item.push(full_crd_data[i][j].clone());
            }
        } else {
            for (j, item) in result_data.iter_mut().enumerate().take(9) {
                item.push(full_crd_data[i][j].clone());
            }
        }
    }

    #[allow(clippy::needless_range_loop)]
    fn add_row_with_sums(
        result_data: &mut [Vec<String>],
        full_crd_data: &[Vec<String>],
        i: usize,
        sum_passed: String,
        sum_skipped: String,
    ) {
        for j in 0..6 {
            result_data[j].push(full_crd_data[i][j].clone());
        }
        result_data[6].push(sum_passed);
        result_data[7].push(full_crd_data[i][7].clone());
        result_data[8].push(sum_skipped);
    }
}

impl From<&StatusSnapshot> for Table {
    fn from(snapshot: &StatusSnapshot) -> Self {
        let headers: Vec<_> = snapshot
            .columns
            .iter()
            .map(|column| vec![column.header.to_string()])
            .collect();

        let status_data = if snapshot
            .columns
            .iter()
            .any(|col| col.condensed_values.is_some())
        {
            headers
                .into_iter()
                .zip(snapshot.columns.iter().map(|column| {
                    if let Some(condensed_fn) = column.condensed_values {
                        condensed_fn(&snapshot.crds)
                    } else {
                        snapshot
                            .crds
                            .iter()
                            .flat_map(|crd| (column.values)(crd))
                            .collect()
                    }
                }))
                .map(|(mut header, data)| {
                    header.extend(data);
                    header
                })
                .collect()
        } else {
            snapshot
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
                })
        };

        let mut table = Builder::from_iter(status_data)
            .index()
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
            condensed_values: None,
            alignment: TextAlignment::Right,
            width: Some(6),
        }
    }

    pub fn failed() -> StatusColumn {
        StatusColumn {
            header: "FAILED".to_string(),
            values: |crd| crd_results(crd, ResultType::Failed),
            condensed_values: None,
            alignment: TextAlignment::Right,
            width: Some(6),
        }
    }

    pub fn skipped() -> StatusColumn {
        StatusColumn {
            header: "SKIPPED".to_string(),
            values: |crd| crd_results(crd, ResultType::Skipped),
            condensed_values: None,
            alignment: TextAlignment::Right,
            width: Some(7),
        }
    }

    pub fn last_update() -> StatusColumn {
        StatusColumn {
            header: "LAST UPDATE".to_string(),
            values: crd_time,
            condensed_values: None,
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

    pub fn condensed_crd_type() -> StatusColumn {
        StatusColumn {
            header: "CRD-TYPE".to_string(),
            values: |_| vec![],
            condensed_values: Some(|crds| {
                let snapshot = StatusSnapshot::new(crds.to_vec());
                snapshot.condense_crd_data()[0].clone()
            }),
            ..Default::default()
        }
    }

    pub fn condensed_test_type() -> StatusColumn {
        StatusColumn {
            header: "TEST-TYPE".to_string(),
            values: |_| vec![],
            condensed_values: Some(|crds| {
                let snapshot = StatusSnapshot::new(crds.to_vec());
                snapshot.condense_crd_data()[1].clone()
            }),
            ..Default::default()
        }
    }

    pub fn condensed_cluster() -> StatusColumn {
        StatusColumn {
            header: "CLUSTER".to_string(),
            values: |_| vec![],
            condensed_values: Some(|crds| {
                let snapshot = StatusSnapshot::new(crds.to_vec());
                snapshot.condense_crd_data()[2].clone()
            }),
            ..Default::default()
        }
    }

    pub fn condensed_arch() -> StatusColumn {
        StatusColumn {
            header: "ARCH".to_string(),
            values: |_| vec![],
            condensed_values: Some(|crds| {
                let snapshot = StatusSnapshot::new(crds.to_vec());
                snapshot.condense_crd_data()[3].clone()
            }),
            ..Default::default()
        }
    }

    pub fn condensed_variant() -> StatusColumn {
        StatusColumn {
            header: "VARIANT".to_string(),
            values: |_| vec![],
            condensed_values: Some(|crds| {
                let snapshot = StatusSnapshot::new(crds.to_vec());
                snapshot.condense_crd_data()[4].clone()
            }),
            ..Default::default()
        }
    }

    pub fn condensed_status() -> StatusColumn {
        StatusColumn {
            header: "STATUS".to_string(),
            values: |_| vec![],
            condensed_values: Some(|crds| {
                let snapshot = StatusSnapshot::new(crds.to_vec());
                snapshot.condense_crd_data()[5].clone()
            }),
            ..Default::default()
        }
    }

    pub fn condensed_passed() -> StatusColumn {
        StatusColumn {
            header: "PASSED".to_string(),
            values: |_| vec![],
            condensed_values: Some(|crds| {
                let snapshot = StatusSnapshot::new(crds.to_vec());
                snapshot.condense_crd_data()[6].clone()
            }),
            ..Default::default()
        }
    }

    pub fn condensed_failed() -> StatusColumn {
        StatusColumn {
            header: "FAILED".to_string(),
            values: |_| vec![],
            condensed_values: Some(|crds| {
                let snapshot = StatusSnapshot::new(crds.to_vec());
                snapshot.condense_crd_data()[7].clone()
            }),
            ..Default::default()
        }
    }

    pub fn condensed_skipped() -> StatusColumn {
        StatusColumn {
            header: "SKIPPED".to_string(),
            values: |_| vec![],
            condensed_values: Some(|crds| {
                let snapshot = StatusSnapshot::new(crds.to_vec());
                snapshot.condense_crd_data()[8].clone()
            }),
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
pub fn crd_type(crd: &Crd) -> Vec<String> {
    match crd {
        Crd::Test(_) => vec!["Test".to_string()],
        Crd::Resource(_) => vec!["Resource".to_string()],
    }
}

/// Determine the state of the CRD
pub fn crd_state(crd: &Crd) -> Vec<String> {
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

pub enum ResultType {
    Passed,
    Failed,
    Skipped,
}

/// Collect the CRD results
pub fn crd_results(crd: &Crd, res_type: ResultType) -> Vec<String> {
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::{Test, TestSpec};
    use kube::api::ObjectMeta;
    use std::collections::BTreeMap;

    fn create_test_crd(
        name: &str,
        namespace: &str,
        labels: Option<&BTreeMap<String, String>>,
        test_spec: TestSpec,
    ) -> Test {
        Test {
            metadata: ObjectMeta {
                name: Some(name.to_owned()),
                namespace: Some(namespace.to_owned()),
                labels: labels.cloned(),
                ..Default::default()
            },
            spec: test_spec,
            status: None,
        }
    }

    #[test]
    fn status_snapshot() {
        let crds = vec![
            Crd::Test(create_test_crd(
                "test1-name",
                "test",
                None,
                TestSpec::default(),
            )),
            Crd::Test(create_test_crd(
                "test2-name",
                "test",
                None,
                TestSpec::default(),
            )),
        ];
        let mut snapshot = StatusSnapshot::new(crds);
        snapshot.add_column(StatusColumn::name());
        snapshot.add_column(StatusColumn::crd_type());
        snapshot.add_column(StatusColumn::state());
        snapshot.add_column(StatusColumn::passed());
        snapshot.add_column(StatusColumn::failed());
        snapshot.add_column(StatusColumn::skipped());

        let snapshot_str = format!("{:width$}", &snapshot, width = 80);
        let expected =
            " NAME             TYPE       STATE             PASSED       FAILED      SKIPPED 
 test1-name       Test       unknown                                            
 test2-name       Test       unknown                                            ";

        println!("{}", snapshot_str);
        assert_eq!(snapshot_str, expected);
    }
}
