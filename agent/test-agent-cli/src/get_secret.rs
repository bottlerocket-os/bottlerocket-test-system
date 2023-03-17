use crate::error::{ClientSnafu, Result, SecretKeyFetchSnafu, SecretMissingSnafu};
use crate::init::TestConfig;
use agent_common::secrets::SecretsReader;
use argh::FromArgs;
use serde_json::{Map, Value};
use snafu::{OptionExt, ResultExt};
use test_agent::{Client, DefaultClient, Spec};

#[derive(Debug, FromArgs, PartialEq)]
#[argh(subcommand, name = "get-secret", description = "Get secret for a key")]
pub(crate) struct GetSecret {
    #[argh(
        positional,
        short = 'k',
        description = "secret key whose value you like to get"
    )]
    secret_key: String,
}

impl GetSecret {
    pub(crate) async fn run(&self, k8s_client: DefaultClient) -> Result<()> {
        let spec: Spec<TestConfig> = k8s_client.spec().await.context(ClientSnafu)?;
        let secret_name = spec
            .secrets
            .get(&self.secret_key)
            .context(SecretKeyFetchSnafu {
                key: &self.secret_key,
            })?;

        let secrets_reader = SecretsReader::new();
        let secret_data: Map<_, _> = secrets_reader
            .get_secret(secret_name)
            .context(SecretMissingSnafu)?
            .into_iter()
            .map(|(key, value)| (key, String::from_utf8_lossy(&value).to_string()))
            .map(|(k, v)| (k, Value::String(v)))
            .collect();

        println!("{}", Value::Object(secret_data));
        Ok(())
    }
}
