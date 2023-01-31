use anyhow::{Context, Error, Result};
use clap::Parser;
use futures::StreamExt;
use testsys_model::test_manager::{ResourceState, TestManager};

/// Restart an object from a testsys cluster.
#[derive(Debug, Parser)]
pub(crate) struct Logs {
    /// The name of the test we want logs from.
    #[clap(long, conflicts_with = "resource")]
    test: Option<String>,

    /// The name of the resource we want logs from.
    #[clap(long, conflicts_with = "test", requires = "state")]
    resource: Option<String>,

    /// The resource state we want logs for (Creation, Destruction).
    #[clap(long = "state", conflicts_with = "test")]
    resource_state: Option<ResourceState>,

    /// Retrieve logs for the testsys controller
    #[clap(long = "controller", conflicts_with_all = &["test", "resource", "state"])]
    controller: bool,

    /// Include logs from dependencies.
    #[clap(long, short)]
    include_resources: bool,

    /// Follow logs
    #[clap(long, short)]
    follow: bool,
}

impl Logs {
    pub(crate) async fn run(self, client: TestManager) -> Result<()> {
        match (self.test, self.resource, self.resource_state, self.controller) {
            (Some(test), None, None, false ) => {
                let mut logs = client.test_logs(test, self.follow).await.context("Unable to get logs.")?;
                while let Some(line) = logs.next().await {
                    print!("{}", String::from_utf8_lossy(&line.context("Unable to read line")?));
                }
            }
            (None, Some(resource), Some(state), false) => {
                let mut logs = client.resource_logs(resource, state, self.follow).await.context("Unable to get logs.")?;
                while let Some(line) = logs.next().await {
                    print!("{}", String::from_utf8_lossy(&line.context("Unable to read line")?));
                }
            }
            (None, None, None, true) => {
                let mut logs = client.controller_logs(self.follow).await.context("Unable to get logs.")?;
                while let Some(line) = logs.next().await {
                    print!("{}", String::from_utf8_lossy(&line.context("Unable to read line")?));
                }
            }
            _ => return Err(Error::msg("Invalid arguments were provided. Exactly one of `--test`, `--resource`, and `--controller` must be used.")),
        };
        Ok(())
    }
}
