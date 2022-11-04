use crate::constants::{DEFAULT_ASSUME_ROLE_SESSION_DURATION, DEFAULT_REGION};
use crate::error::{self, Error};
use agent_common::secrets::SecretsReader;
use aws_config::default_provider::credentials::default_provider;
use aws_config::sts::AssumeRoleProvider;
use aws_config::RetryConfig;
use aws_sdk_sts::Region;
use aws_smithy_types::retry::RetryMode;
use aws_types::credentials::{Credentials, SharedCredentialsProvider};
use aws_types::SdkConfig;
use log::info;
use model::SecretName;
use snafu::{OptionExt, ResultExt};
use std::env;
use std::time::Duration;

/// Set up the config for aws calls using `aws_secret_name` if provided and `sts::assume_role`
/// if a role arn is provided. Set credentials as environment variables if `setup_env` is true
pub async fn aws_config(
    aws_secret_name: &Option<&SecretName>,
    assume_role: &Option<String>,
    assume_role_session_duration: &Option<i32>,
    region: &Option<String>,
    setup_env: bool,
) -> Result<SdkConfig, Error> {
    let region = region
        .as_ref()
        .unwrap_or(&DEFAULT_REGION.to_string())
        .to_string();
    info!(
        "Creating a custom region provider for '{}' to be used in the aws config.",
        region
    );

    let mut config_loader = aws_config::from_env().retry_config(
        RetryConfig::standard()
            .with_retry_mode(RetryMode::Adaptive)
            .with_max_attempts(15),
    );
    let base_provider = match aws_secret_name {
        Some(aws_secret_name) => {
            let (access_key_id, secret_access_key, session_token) =
                get_secret_values(aws_secret_name)?;
            if setup_env && assume_role.is_none() {
                set_environment_variables(&access_key_id, &secret_access_key, &session_token);
            }
            SharedCredentialsProvider::new(Credentials::new(
                access_key_id,
                secret_access_key,
                session_token,
                None,
                "aws_secret",
            ))
        }
        None => SharedCredentialsProvider::new(default_provider().await),
    };

    config_loader = match assume_role {
        Some(role_arn) => config_loader.credentials_provider(SharedCredentialsProvider::new(
            AssumeRoleProvider::builder(role_arn)
                .region(Region::new(region.clone()))
                .session_name("testsys")
                .session_length(Duration::from_secs(
                    assume_role_session_duration.unwrap_or(DEFAULT_ASSUME_ROLE_SESSION_DURATION)
                        as u64,
                ))
                .build(base_provider.clone()),
        )),
        None => config_loader.credentials_provider(base_provider),
    };

    let config = config_loader
        .region(Region::new(region.clone()))
        .load()
        .await;
    if let (Some(role_arn), true) = (assume_role, setup_env) {
        info!("Getting credentials for assumed role '{}'.", role_arn);
        let sts_client = aws_sdk_sts::Client::new(&config);
        let credentials = sts_client
            .assume_role()
            .role_arn(role_arn)
            .role_session_name("testsys")
            .set_duration_seconds(*assume_role_session_duration)
            .send()
            .await
            .context(error::AssumeRoleSnafu { role_arn })?
            .credentials()
            .context(error::CredentialsMissingSnafu { role_arn })?
            .clone();
        set_environment_variables(
            &String::from(
                credentials
                    .access_key_id()
                    .context(error::CredentialsMissingSnafu { role_arn })?,
            ),
            &String::from(
                credentials
                    .secret_access_key()
                    .context(error::CredentialsMissingSnafu { role_arn })?,
            ),
            &Some(String::from(
                credentials
                    .session_token()
                    .context(error::CredentialsMissingSnafu { role_arn })?,
            )),
        )
    }
    Ok(config)
}

fn get_secret_values(
    aws_secret_name: &SecretName,
) -> Result<(String, String, Option<String>), Error> {
    let reader = SecretsReader::new();
    let aws_secret = reader
        .get_secret(aws_secret_name)
        .context(error::SecretMissingSnafu)?;
    let access_key_id = String::from_utf8(
        aws_secret
            .get("access-key-id")
            .context(error::EnvSetupSnafu {
                what: format!("access-key-id missing from secret '{}'", aws_secret_name),
            })?
            .to_owned(),
    )
    .context(error::ConversionSnafu {
        what: "access-key-id",
    })?;
    let secret_access_key = String::from_utf8(
        aws_secret
            .get("secret-access-key")
            .context(error::EnvSetupSnafu {
                what: format!(
                    "secret-access-key missing from secret '{}'",
                    aws_secret_name
                ),
            })?
            .to_owned(),
    )
    .context(error::ConversionSnafu {
        what: "access-key-id",
    })?;
    let session_token = match aws_secret.get("session-token") {
        Some(token) => Some(String::from_utf8(token.to_owned()).context(
            error::ConversionSnafu {
                what: "session-token",
            },
        )?),
        None => None,
    };
    Ok((access_key_id, secret_access_key, session_token))
}

fn set_environment_variables(
    access_key_id: &String,
    secret_access_key: &String,
    session_token: &Option<String>,
) {
    env::set_var("AWS_ACCESS_KEY_ID", access_key_id);
    env::set_var("AWS_SECRET_ACCESS_KEY", secret_access_key);
    if let Some(session_token) = session_token {
        env::set_var("AWS_SESSION_TOKEN", session_token);
    }
}
