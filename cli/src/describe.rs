use anyhow::{Error, Result};
use clap::Parser;
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
                let description = client.describe_test(test).await?;
                println!("{}", description);
            }
            (None, Some(resource)) => {
                let description = client.describe_resource(resource).await?;
                println!("{}", description);
            }
            _ => return Err(Error::msg("Invalid arguments were provided. Exactly one of `--test` and `--resource` must be used.")),
        };
        Ok(())
    }
}
