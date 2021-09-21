use crate::add_file;
use crate::error::Result;
use kube::Client;
use structopt::StructOpt;

/// Add a resource provider to a testsys cluster.
#[derive(Debug, StructOpt)]
pub(crate) struct Add {
    #[structopt(subcommand)]
    command: Command,
}

#[derive(Debug, StructOpt)]
enum Command {
    /// Add a resource provider from a YAML file.
    File(add_file::AddFile),
}

impl Add {
    pub(crate) async fn run(&self, k8s_client: Client) -> Result<()> {
        match &self.command {
            Command::File(add_file) => add_file.run(k8s_client).await,
        }
    }
}
