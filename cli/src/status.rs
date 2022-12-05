use anyhow::{Context, Result};
use clap::Parser;
use model::test_manager::{StatusProgress, TestManager};
use terminal_size::{Height, Width};

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
    #[clap(long, short = 't', requires("progress"))]
    with_test: bool,

    /// Include the time the CRD was last updated
    #[clap(long, short = 'u')]
    with_time: bool,
}

impl Status {
    pub(crate) async fn run(self, client: TestManager) -> Result<()> {
        let mut status = client
            .status(&Default::default(), self.controller)
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
