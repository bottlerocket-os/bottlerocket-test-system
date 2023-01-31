use anyhow::{Context, Result};
use clap::{value_parser, Parser};
use std::path::PathBuf;
use testsys_model::test_manager::TestManager;

/// Retrieve the results of a test.
#[derive(Debug, Parser)]
pub(crate) struct Results {
    /// Name of the sonobuoy test.
    #[clap(short = 'n', long)]
    test_name: String,
    /// The place the test results should be written (results.tar.gz)
    #[clap(long, value_parser = value_parser!(PathBuf), default_value = "results.tar.gz")]
    destination: PathBuf,
}

impl Results {
    pub(crate) async fn run(&self, client: TestManager) -> Result<()> {
        client
            .write_test_results(&self.test_name, &self.destination)
            .await
            .context("Unable to write results")
    }
}
