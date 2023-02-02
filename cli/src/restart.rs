use crate::restart_test;
use anyhow::Result;
use clap::Parser;
use testsys_model::test_manager::TestManager;

/// Restart testsys tests.
#[derive(Debug, Parser)]
pub(crate) struct Restart {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Parser)]
enum Command {
    /// Restart a testsys test.
    Test(restart_test::RestartTest),
}

impl Restart {
    pub(crate) async fn run(self, client: TestManager) -> Result<()> {
        match self.command {
            Command::Test(restart_test) => restart_test.run(client).await,
        }
    }
}
