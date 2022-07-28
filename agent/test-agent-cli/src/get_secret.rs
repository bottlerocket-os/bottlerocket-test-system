use crate::error::{Result, SecretKeyFetchSnafu};
use crate::Client;
use argh::FromArgs;
use snafu::OptionExt;

#[derive(Debug, FromArgs, PartialEq)]
#[argh(subcommand, name = "get_secret", description = "Get secret for a key")]
pub(crate) struct GetSecret {
    #[argh(
        option,
        short = 'k',
        description = "secret key whose value you like to get"
    )]
    secret_key: String,
}

impl GetSecret {
    pub(crate) async fn run(&self, k8s_client: Client) -> Result<()> {
        let spec = k8s_client.spec().await?;
        let secret = spec
            .secrets
            .get(&self.secret_key)
            .context(SecretKeyFetchSnafu {
                key: &self.secret_key,
            })?;
        println!("{}", secret);
        Ok(())
    }
}
