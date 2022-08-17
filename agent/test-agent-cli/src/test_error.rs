use crate::error::{ClientSnafu, Result};
use argh::FromArgs;
use snafu::ResultExt;
use test_agent::{Client, DefaultClient};

#[derive(Debug, FromArgs, PartialEq)]
#[argh(
    subcommand,
    name = "send-error",
    description = "send error encountered"
)]
pub(crate) struct TestError {
    #[argh(positional, description = "error message")]
    error: String,
}

impl TestError {
    pub(crate) async fn run(&self, k8s_client: DefaultClient) -> Result<()> {
        k8s_client
            .send_error(&self.error)
            .await
            .context(ClientSnafu)?;
        Ok(())
    }
}
