use crate::run_file;
use anyhow::Result;
use clap::Parser;
use model::test_manager::TestManager;

/// Run testsys tests.
#[derive(Debug, Parser)]
pub(crate) struct Run {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Parser)]
enum Command {
    /// Run a test from a YAML file.
    File(run_file::RunFile),
}

impl Run {
    pub(crate) async fn run(self, client: TestManager) -> Result<()> {
        match self.command {
            Command::File(run_file) => run_file.run(client).await,
        }
    }
}
