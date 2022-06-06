use crate::error::Result;
use crate::{add_aws_secret, add_secret};
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
    /// Add a secret to the cluster.
    Secret(add_secret::AddSecret),

    /// Add AWS credentials as a secret.
    AwsSecret(add_aws_secret::AddAwsSecret),
}

impl Add {
    pub(crate) async fn run(&self, k8s_client: Client) -> Result<()> {
        match &self.command {
            Command::Secret(add_secret) => add_secret.run(k8s_client).await,
            Command::AwsSecret(add_aws_secret) => add_aws_secret.run(k8s_client).await,
        }
    }
}
