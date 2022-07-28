use crate::error::{JsonSerializeSnafu, Result};
use crate::Client;
use argh::FromArgs;
use snafu::ResultExt;

#[derive(Debug, FromArgs, PartialEq)]
#[argh(
    subcommand,
    name = "init",
    description = "set task_state running and get the configuration details required for test"
)]
pub(crate) struct Init {}

impl Init {
    pub(crate) async fn run(&self, k8s_client: Client) -> Result<()> {
        k8s_client.send_test_starting().await?;
        let spec = k8s_client.spec().await?;
        let config = spec.configuration;
        println!(
            "{}",
            serde_json::to_string(&config).context(JsonSerializeSnafu)?
        );
        Ok(())
    }
}
