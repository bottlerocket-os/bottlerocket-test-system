use crate::error::Result;
use crate::k8s::create_or_update;
use k8s_openapi::api::core::v1::Secret;
use kube::{Api, Client};
use model::constants::NAMESPACE;
use model::SecretName;
use std::collections::BTreeMap;
use structopt::StructOpt;

/// Add a `Secret` with key value pairs.
#[derive(Debug, StructOpt)]
pub(crate) struct AddAwsSecret {
    /// Name of the secret
    #[structopt(short, long)]
    name: SecretName,

    /// Aws access key id.
    #[structopt(short = "u", long)]
    aws_access_key_id: String,

    /// Aws secret access key.
    #[structopt(short = "p", long)]
    secret_access_key: String,
}

impl AddAwsSecret {
    pub(crate) async fn run(&self, k8s_client: Client) -> Result<()> {
        let args: BTreeMap<String, String> = vec![
            ("access-key-id".to_string(), self.aws_access_key_id.clone()),
            (
                "secret-access-key".to_string(),
                self.secret_access_key.clone(),
            ),
        ]
        .into_iter()
        .collect();

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
        println!("Successfully added '{}' to secrets.", self.name);
        Ok(())
    }
}
