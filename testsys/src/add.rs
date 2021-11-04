use crate::error::Result;
use crate::{add_file, add_secret};
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

    /// Add a secret to the cluster.
    Secret(add_secret::AddSecret),
}

impl Add {
    pub(crate) async fn run(&self, k8s_client: Client) -> Result<()> {
        match &self.command {
            Command::File(add_file) => add_file.run(k8s_client).await,
            Command::Secret(add_secret) => add_secret.run(k8s_client).await,
        }
    }
}
