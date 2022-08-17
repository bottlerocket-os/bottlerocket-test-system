use crate::error::{ClientSnafu, Result};
use argh::FromArgs;
use snafu::ResultExt;
use test_agent::{Client, DefaultClient};

#[derive(Debug, FromArgs, PartialEq)]
#[argh(
    subcommand,
    name = "retry-count",
    description = "Number of retries allowed in case of failing test"
)]
pub(crate) struct RetryCount {}

impl RetryCount {
    pub(crate) async fn run(&self, k8s_client: DefaultClient) -> Result<()> {
        let retries = k8s_client.retries().await.context(ClientSnafu)?;
        println!("{}", &retries);
        Ok(())
    }
}
