use anyhow::{Context, Result};
use clap::Parser;
use testsys_model::test_manager::TestManager;

/// Restart an object from a testsys cluster.
#[derive(Debug, Parser)]
pub(crate) struct RestartTest {
    /// The name of the test to be restarted.
    #[clap()]
    test_name: String,
}

impl RestartTest {
    pub(crate) async fn run(self, client: TestManager) -> Result<()> {
        client
            .restart_test(&self.test_name)
            .await
            .context("Unable to restart the test")
    }
}
