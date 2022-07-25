use crate::error::Result;
use crate::Client;
use argh::FromArgs;

#[derive(Debug, FromArgs, PartialEq)]
#[argh(subcommand, name = "error", description = "send error encountered")]
pub(crate) struct TestError {
    #[argh(long = "error", short = 'e', option, description = "error message")]
    error: String,
}

impl TestError {
    pub(crate) async fn run(&self, k8s_client: Client) -> Result<()> {
        k8s_client.send_error(&self.error).await?;
        Ok(())
    }
}
