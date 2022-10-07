use crate::constants::DEFAULT_REGION;
use crate::error::{self, Error};
use aws_config::meta::region::RegionProviderChain;
use aws_config::RetryConfig;
use aws_sdk_sts::Region;
use aws_smithy_types::retry::RetryMode;
use aws_types::SdkConfig;
use log::info;
use model::SecretName;
use resource_agent::clients::InfoClient;
use resource_agent::provider::{ProviderResult, Resources};
use snafu::{OptionExt, ResultExt};
use std::env;
use test_agent::Runner;

/// Set up the config for aws calls using `aws_secret_name` if provided and `sts::assume_role`
/// if a role arn is provided.
pub async fn aws_test_config<R>(
    runner: &R,
    aws_secret_name: &Option<SecretName>,
    assume_role: &Option<String>,
    assume_role_session_duration: &Option<i32>,
    region: &Option<String>,
) -> Result<SdkConfig, Error>
where
    R: Runner,
{
    if let Some(aws_secret_name) = aws_secret_name {
        info!("Adding secret '{}' to the environment", aws_secret_name);
        setup_test_env(runner, aws_secret_name).await?;
    }

    let region = region
        .as_ref()
        .unwrap_or(&DEFAULT_REGION.to_string())
        .to_string();
    info!(
        "Creating a custom region provider for '{}' to be used in the aws config.",
        region
    );
    let region_provider = RegionProviderChain::first_try(Region::new(region.clone()));

    let mut config = aws_config::from_env()
        .retry_config(
            RetryConfig::standard()
                .with_retry_mode(RetryMode::Adaptive)
                .with_max_attempts(15),
        )
        .region(region_provider)
        .load()
        .await;

    if let Some(role_arn) = assume_role {
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
        // Set the env variables for our assumed role.
        env::set_var(
            "AWS_ACCESS_KEY_ID",
            credentials
                .access_key_id()
                .context(error::CredentialsMissingSnafu { role_arn })?,
        );
        env::set_var(
            "AWS_SECRET_ACCESS_KEY",
            credentials
                .secret_access_key()
                .context(error::CredentialsMissingSnafu { role_arn })?,
        );
        env::set_var(
            "AWS_SESSION_TOKEN",
            credentials
                .session_token()
                .context(error::CredentialsMissingSnafu { role_arn })?,
        );
        let region_provider = RegionProviderChain::first_try(Region::new(region.clone()));
        config = aws_config::from_env()
            .retry_config(
                RetryConfig::standard()
                    .with_retry_mode(RetryMode::Adaptive)
                    .with_max_attempts(15),
            )
            .region(region_provider)
            .load()
            .await;
    }

    Ok(config)
}

/// Set up AWS credential secrets in a runner's process's environment
pub async fn setup_test_env<R>(runner: &R, aws_secret_name: &SecretName) -> Result<(), Error>
where
    R: Runner,
{
    let aws_secret = runner
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

    if let Some(token) = aws_secret.get("session-token") {
        env::set_var(
            "AWS_SESSION_TOKEN",
            String::from_utf8(token.to_owned()).context(error::ConversionSnafu {
                what: "session-token",
            })?,
        );
    };

    env::set_var("AWS_ACCESS_KEY_ID", access_key_id);
    env::set_var("AWS_SECRET_ACCESS_KEY", secret_access_key);

    Ok(())
}

/// Set up the config for aws calls using `aws_secret_name` if provided and `sts::assume_role`
/// if a role arn is provided.
pub async fn aws_resource_config<I>(
    client: &I,
    aws_secret_name: &Option<&SecretName>,
    assume_role: &Option<String>,
    region: &Option<String>,
    resources: Resources,
) -> ProviderResult<SdkConfig>
where
    I: InfoClient,
{
    let region = region
        .as_ref()
        .unwrap_or(&DEFAULT_REGION.to_string())
        .to_string();

    if let Some(aws_secret_name) = aws_secret_name {
        info!("Adding secret '{}' to the environment", aws_secret_name);
        setup_resource_env(client, aws_secret_name, resources).await?;
    }
    let region_provider = RegionProviderChain::first_try(Region::new(region.clone()));
    let mut config = aws_config::from_env()
        .retry_config(
            RetryConfig::standard()
                .with_retry_mode(RetryMode::Adaptive)
                .with_max_attempts(15),
        )
        .region(region_provider)
        .load()
        .await;

    if let Some(role_arn) = assume_role {
        info!("Getting credentials for assumed role '{}'.", role_arn);
        let sts_client = aws_sdk_sts::Client::new(&config);
        let assume_role_output = resource_agent::provider::IntoProviderError::context(
            sts_client
                .assume_role()
                .role_arn(role_arn)
                .role_session_name("testsys")
                .send()
                .await,
            resources,
            format!("Unable to get credentials for role '{}'", role_arn),
        )?;
        let credentials = resource_agent::provider::IntoProviderError::context(
            assume_role_output.credentials(),
            resources,
            format!("Credentials missing for assumed role '{}'", role_arn),
        )?;
        // Set the env variables for our assumed role.
        env::set_var(
            "AWS_ACCESS_KEY_ID",
            resource_agent::provider::IntoProviderError::context(
                credentials.access_key_id(),
                resources,
                "Credentials missing `access_key_id`",
            )?,
        );
        env::set_var(
            "AWS_SECRET_ACCESS_KEY",
            resource_agent::provider::IntoProviderError::context(
                credentials.secret_access_key(),
                resources,
                "Credentials missing `secret_access_key`",
            )?,
        );
        env::set_var(
            "AWS_SESSION_TOKEN",
            resource_agent::provider::IntoProviderError::context(
                credentials.session_token(),
                resources,
                "Credentials missing `session_token`",
            )?,
        );
        let region_provider = RegionProviderChain::first_try(Region::new(region.clone()));
        config = aws_config::from_env()
            .retry_config(
                RetryConfig::standard()
                    .with_retry_mode(RetryMode::Adaptive)
                    .with_max_attempts(15),
            )
            .region(region_provider)
            .load()
            .await;
    }

    Ok(config)
}

/// Set up AWS credential secrets in a resource's process's environment
pub async fn setup_resource_env<I>(
    client: &I,
    aws_secret_name: &SecretName,
    resources: Resources,
) -> ProviderResult<()>
where
    I: InfoClient,
{
    let aws_secret = resource_agent::provider::IntoProviderError::context(
        client.get_secret(aws_secret_name).await,
        resources,
        format!("Error getting secret '{}'", aws_secret_name),
    )?;

    let access_key_id = resource_agent::provider::IntoProviderError::context(
        String::from_utf8(
            resource_agent::provider::IntoProviderError::context(
                aws_secret.get("access-key-id"),
                resources,
                format!("access-key-id missing from secret '{}'", aws_secret_name),
            )?
            .to_owned(),
        ),
        resources,
        "Could not convert access-key-id to String",
    )?;
    let secret_access_key = resource_agent::provider::IntoProviderError::context(
        String::from_utf8(
            resource_agent::provider::IntoProviderError::context(
                aws_secret.get("secret-access-key"),
                resources,
                format!(
                    "secret-access-key missing from secret '{}'",
                    aws_secret_name
                ),
            )?
            .to_owned(),
        ),
        resources,
        "Could not convert secret-access-key to String",
    )?;
    if let Some(token) = aws_secret.get("session-token") {
        env::set_var(
            "AWS_SESSION_TOKEN",
            resource_agent::provider::IntoProviderError::context(
                String::from_utf8(token.to_owned()),
                resources,
                "Could not convert session-token to String",
            )?,
        );
    };

    env::set_var("AWS_ACCESS_KEY_ID", access_key_id);
    env::set_var("AWS_SECRET_ACCESS_KEY", secret_access_key);

    Ok(())
}
