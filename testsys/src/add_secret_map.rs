use crate::error::{self, Result};
use crate::k8s::create_or_update;
use k8s_openapi::api::core::v1::Secret;
use kube::{Api, Client};
use model::constants::NAMESPACE;
use model::SecretName;
use snafu::OptionExt;
use std::collections::BTreeMap;
use structopt::StructOpt;

/// Add a `Secret` with key value pairs.
#[derive(Debug, StructOpt)]
pub(crate) struct AddSecretMap {
    /// Name of the secret
    #[structopt(short, long)]
    name: SecretName,

    /// Key value pairs for secrets. (Key=value)
    #[structopt(parse(try_from_str = parse_key_val))]
    args: Vec<(String, String)>,
}

impl AddSecretMap {
    pub(crate) async fn run(&self, k8s_client: Client) -> Result<()> {
        let args: BTreeMap<String, String> = self.args.clone().into_iter().collect();

        let secrets: Api<k8s_openapi::api::core::v1::Secret> =
            Api::namespaced(k8s_client.clone(), NAMESPACE);

        let object_meta = kube::api::ObjectMeta {
            name: Some(self.name.as_str().to_owned()),
            ..Default::default()
        };

        // Create the secret we are going to add.
        let secret = Secret {
            data: None,
            immutable: None,
            metadata: object_meta,
            string_data: Some(args),
            type_: None,
        };

        create_or_update(&secrets, secret, "Secret").await?;
        Ok(())
    }
}

fn parse_key_val(s: &str) -> Result<(String, String)> {
    let mut iter = s.splitn(2, '=');
    let key = iter
        .next()
        .context(error::ArgumentMissing { arg: s.to_string() })?;
    let value = iter
        .next()
        .context(error::ArgumentMissing { arg: s.to_string() })?;
    Ok((key.to_string(), value.to_string()))
}
