use crate::error::Result;
use crate::{run_aws_ecs, run_aws_k8s, run_file, run_sonobuoy, run_vmware};
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
    Sonobuoy(Box<run_sonobuoy::RunSonobuoy>),
    /// Create an EKS resource, an EC2 resource, and run a Sonobuoy test. This test mode is useful
    /// for the `aws-k8s` variants of Bottlerocket.
    AwsK8s(Box<run_aws_k8s::RunAwsK8s>),
    /// Create an ECS resource, an EC2 resource, and run an ECS task. This test mode is useful
    /// for the `aws-ecs` variants of Bottlerocket.
    AwsEcs(Box<run_aws_ecs::RunAwsEcs>),
    /// Create VM nodes on a cluster running in vSphere and run a sonobuoy test. This test mode is
    /// useful for the `vmware` variants of Bottlerocket.
    Vmware(Box<run_vmware::RunVmware>),
}

impl Run {
    pub(crate) async fn run(self, k8s_client: Client) -> Result<()> {
        match self.command {
            Command::File(run_file) => run_file.run(k8s_client).await,
            Command::Sonobuoy(run_sonobuoy) => run_sonobuoy.run(k8s_client).await,
            Command::AwsK8s(run_aws_k8s) => run_aws_k8s.run(k8s_client).await,
            Command::AwsEcs(run_aws_ecs) => run_aws_ecs.run(k8s_client).await,
            Command::Vmware(run_vmware) => run_vmware.run(k8s_client).await,
        }
    }
}
