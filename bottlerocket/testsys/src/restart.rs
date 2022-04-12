use crate::error::Result;
use crate::restart_test;
use kube::Client;
use structopt::StructOpt;

/// Restart testsys tests.
#[derive(Debug, StructOpt)]
pub(crate) struct Restart {
    #[structopt(subcommand)]
    command: Command,
}

#[derive(Debug, StructOpt)]
enum Command {
    /// Restart a testsys test.
    Test(restart_test::RestartTest),
}

impl Restart {
    pub(crate) async fn run(self, k8s_client: Client) -> Result<()> {
        match self.command {
            Command::Test(restart_test) => restart_test.run(k8s_client).await,
        }
    }
}
