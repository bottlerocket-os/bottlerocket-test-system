use anyhow::{Context, Error, Result};
use clap::Parser;
use futures::StreamExt;
use model::test_manager::TestManager;

/// Retrieve the YAML description of a test or resource.
#[derive(Debug, Parser)]
pub(crate) struct Describe {
    /// The name of the test we want the description from.
    #[clap(long, conflicts_with = "resource")]
    test: Option<String>,

    /// The name of the resource we want the description from.
    #[clap(long, conflicts_with = "test")]
    resource: Option<String>,
}

impl Describe {
    pub(crate) async fn run(self, client: TestManager) -> Result<()> {
        match (self.test, self.resource) {
            (Some(test), None) => {
                let mut logs = client.describe_test(test).await.context("Unable to get test description.")?;
                while let Some(line) = logs.next().await {
                    print!("{}", String::from_utf8_lossy(&line.context("Unable to read line")?));
                }
            }
            (None, Some(resource)) => {
                let mut logs = client.describe_resource(resource).await.context("Unable to get resource description.")?;
                while let Some(line) = logs.next().await {
                    print!("{}", String::from_utf8_lossy(&line.context("Unable to read line")?));
                }
            }
            _ => return Err(Error::msg("Invalid arguments were provided. Exactly one of `--test` and `--resource` must be used.")),
        };
        Ok(())
    }
}
