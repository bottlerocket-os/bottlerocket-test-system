use anyhow::{Error, Result};
use clap::Parser;
use model::clients::CrdClient;
use model::test_manager::TestManager;
use model::CrdExt;

/// Retrieve the YAML description of a test or resource.
#[derive(Debug, Parser)]
pub(crate) struct Describe {
    /// The name of the test to return the description from.
    #[clap(long, conflicts_with = "resource")]
    test: Option<String>,

    /// The name of the resource to return the description from.
    #[clap(long, conflicts_with = "test")]
    resource: Option<String>,
}

impl Describe {
    pub(crate) async fn run(self, client: TestManager) -> Result<()> {
        match (self.test, self.resource) {
            (Some(test), None) => {
                let test_yaml = client.test_client().get(test).await?.to_yaml()?;
                println!("{}", test_yaml);
            }
            (None, Some(resource)) => {
                let resource_yaml = client.resource_client().get(resource).await?.to_yaml()?;
                println!("{}", resource_yaml);
            }
            _ => return Err(Error::msg("Invalid arguments were provided. Exactly one of `--test` and `--resource` must be used.")),
        };
        Ok(())
    }
}
