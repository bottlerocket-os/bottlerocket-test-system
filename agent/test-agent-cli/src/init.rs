use crate::error::{ArchiveSnafu, ClientSnafu, JsonSerializeSnafu, Result};
use argh::FromArgs;
use model::constants::TESTSYS_RESULTS_DIRECTORY;
use model::Configuration;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use snafu::ResultExt;
use std::fs;
use test_agent::{Client, DefaultClient, Spec};

#[derive(Debug, FromArgs, PartialEq)]
#[argh(
    subcommand,
    name = "init",
    description = "set task_state running and get the configuration details required for test"
)]
pub(crate) struct Init {}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TestConfig {
    #[serde(flatten)]
    config: Map<String, Value>,
}

impl Configuration for TestConfig {}

impl Init {
    pub(crate) async fn run(&self, k8s_client: DefaultClient) -> Result<()> {
        k8s_client.send_test_starting().await.context(ClientSnafu)?;

        fs::create_dir_all(TESTSYS_RESULTS_DIRECTORY).context(ArchiveSnafu)?;
        let spec: Spec<TestConfig> = k8s_client.spec().await.context(ClientSnafu)?;

        println!(
            "{}",
            serde_json::to_string(&spec.configuration).context(JsonSerializeSnafu)?
        );
        Ok(())
    }
}
