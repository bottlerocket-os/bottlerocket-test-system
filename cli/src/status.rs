use anyhow::{Context, Result};
use clap::Parser;
use terminal_size::{Height, Width};
use testsys_model::test_manager::{
    CrdState, CrdType, SelectionParams, StatusProgress, TestManager,
};

/// Check the status of a TestSys object.
#[derive(Debug, Parser)]
pub(crate) struct Status {
    /// Output the results in JSON format.
    #[clap(long = "json")]
    json: bool,

    /// Check the status of the testsys controller
    #[clap(long, short = 'c')]
    controller: bool,

    /// Include the status of resources when reporting status
    #[clap(long, short = 'p')]
    progress: bool,

    /// Include the `Test` status, too. Requires `--progress`
    #[clap(long, requires("progress"))]
    with_test: bool,

    /// Include the time the CRD was last updated
    #[clap(long, short = 'u')]
    with_time: bool,

    /// Include `Test`s (if passed with `--resources`, `Test`s and `Resource`s will be shown)
    #[clap(long, short = 't')]
    tests: bool,

    /// Include `Resource`s (if passed with `--tests`, `Test`s and `Resource`s will be shown)
    #[clap(long, short = 'r')]
    resources: bool,

    /// Only include objects with the specified labels ("foo=bar,biz=baz")
    #[clap(long)]
    labels: Option<String>,

    /// Only include objects with the specified state ("completed", "running", "not-finished",
    /// "passed", "failed")
    #[clap(long)]
    state: Option<CrdState>,

    /// Only include objects with the specified name
    #[clap(long)]
    name: Option<String>,
}

impl Status {
    pub(crate) async fn run(self, client: TestManager) -> Result<()> {
        let crd_type = match (self.tests, self.resources) {
            (true, false) => Some(CrdType::Test),
            (false, true) => Some(CrdType::Resource),
            _ => None,
        };
        let selection_params = SelectionParams {
            crd_type,
            labels: self.labels,
            name: self.name,
            state: self.state,
        };
        let mut status = client
            .status(&selection_params, self.controller)
            .await
            .context("Unable to get status")?;

        if self.with_test {
            status.with_progress(StatusProgress::WithTests);
        } else if self.progress {
            status.with_progress(StatusProgress::Resources);
        }

        if self.with_time {
            status.with_time();
        }

        if self.json {
            println!(
                "{}",
                serde_json::to_string_pretty(&status)
                    .context("Could not create string from status.")?
            );
        } else {
            let (terminal_size::Width(width), _) =
                terminal_size::terminal_size().unwrap_or((Width(120), Height(0)));
            println!("{:width$}", status, width = width as usize);
        }
        Ok(())
    }
}
