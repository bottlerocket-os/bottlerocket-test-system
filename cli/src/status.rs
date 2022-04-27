use anyhow::{Context, Result};
use clap::Parser;
use model::test_manager::TestManager;
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
}

impl Status {
    pub(crate) async fn run(self, client: TestManager) -> Result<()> {
        let status = client
            .status(&Default::default(), self.controller)
            .await
            .context("Unable to get status")?;

        if self.json {
            println!(
                "{}",
                serde_json::to_string_pretty(&status)
                    .context("Could not create string from status.")?
            );
        } else {
            let (terminal_size::Width(width), _) =
                terminal_size::terminal_size().unwrap_or((Width(120), Height(0)));
            println!("{}", status.to_string(width as usize));
        }
        Ok(())
    }
}
