use crate::error::{
    ClientSnafu, ConfMissingSnafu, ConversionSnafu, Result, SecretKeyFetchSnafu, SecretMissingSnafu,
};
use crate::init::TestConfig;
use agent_common::secrets::SecretsReader;
use argh::FromArgs;
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
        let secret_data = secrets_reader
            .get_secret(secret_name)
            .context(SecretMissingSnafu)?;

        let secret = String::from_utf8(
            secret_data
                .get(secret_name.as_str())
                .context(ConfMissingSnafu)?
                .to_owned(),
        )
        .context(ConversionSnafu {
            what: "secret_name",
        })?;

        println!("{:#?}", secret);
        Ok(())
    }
}
