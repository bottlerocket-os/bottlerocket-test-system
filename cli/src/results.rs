use anyhow::{Context, Result};
use clap::Parser;
use model::test_manager::TestManager;
use std::path::PathBuf;

/// Retrieve the results of a test.
#[derive(Debug, Parser)]
pub(crate) struct Results {
    /// Name of the sonobuoy test.
    #[clap(short = 'n', long)]
    test_name: String,
    /// The place the test results should be written (results.tar.gz)
    #[clap(long, parse(from_os_str), default_value = "results.tar.gz")]
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
