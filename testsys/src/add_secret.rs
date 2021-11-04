use crate::add_secret_map;
use crate::error::Result;
use kube::Client;
use structopt::StructOpt;

/// Add a `Secret` to a testsys cluster.
#[derive(Debug, StructOpt)]
pub(crate) struct AddSecret {
    #[structopt(subcommand)]
    command: Command,
}

#[derive(Debug, StructOpt)]
enum Command {
    /// Add a `Secret` to the cluster using key value pairs.
    Map(add_secret_map::AddSecretMap),
}

impl AddSecret {
    pub(crate) async fn run(&self, k8s_client: Client) -> Result<()> {
        match &self.command {
            Command::Map(add_secret_map) => add_secret_map.run(k8s_client).await,
        }
    }
}
