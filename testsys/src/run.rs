use crate::error::Result;
use crate::run_file;
use crate::run_sonobuoy;
use kube::Client;
use structopt::StructOpt;

/// Run testsys tests.
#[derive(Debug, StructOpt)]
pub(crate) struct Run {
    #[structopt(subcommand)]
    command: Command,
}

#[derive(Debug, StructOpt)]
enum Command {
    /// Run a test from a YAML file.
    File(run_file::RunFile),
    /// Run a Sonobuoy test;.
    Sonobuoy(run_sonobuoy::RunSonobuoy),
}

impl Run {
    pub(crate) async fn run(&self, k8s_client: Client) -> Result<()> {
        match &self.command {
            Command::File(run_file) => run_file.run(k8s_client).await,
            Command::Sonobuoy(run_sonobuoy) => run_sonobuoy.run(k8s_client).await,
        }
    }
}
