/*!

`bottlerocket-agents` is a collection of test agent and resource agent implementations used to test
Bottlerocket instances.
This `lib.rs` provides code that is used by multiple agent binaries orused by the `testsys` CLI.

!*/

pub mod error;
pub mod sonobuoy;
pub mod wireguard;

use crate::error::Error;
use aws_config::meta::region::RegionProviderChain;
use aws_config::RetryConfig;
use aws_sdk_ec2::Region;
use aws_smithy_types::retry::RetryMode;
use aws_types::SdkConfig;
use env_logger::Builder;
use log::{info, LevelFilter};
use model::SecretName;
use resource_agent::clients::InfoClient;
use resource_agent::provider::{ProviderError, ProviderResult, Resources};
use serde::Serialize;
use snafu::{OptionExt, ResultExt};
use std::path::Path;
use std::process::Output;
use std::{env, fs};
use test_agent::Runner;

pub const DEFAULT_AGENT_LEVEL_FILTER: LevelFilter = LevelFilter::Info;
pub const DEFAULT_TASK_DEFINITION: &str = "testsys-bottlerocket-aws-default-ecs-smoke-test-v1";
pub const TEST_CLUSTER_KUBECONFIG_PATH: &str = "/local/test-cluster.kubeconfig";
pub const DEFAULT_REGION: &str = "us-west-2";

/// Decode and write out the kubeconfig file for a test cluster to a specified path
pub async fn decode_write_kubeconfig(
    kubeconfig_base64: &str,
    kubeconfig_path: &str,
) -> Result<(), error::Error> {
    let kubeconfig_path = Path::new(kubeconfig_path);
    info!("Decoding kubeconfig for test cluster");
    let decoded_bytes = base64::decode(kubeconfig_base64.as_bytes())
        .context(error::Base64DecodeSnafu { what: "kubeconfig" })?;
    info!("Storing kubeconfig in {}", kubeconfig_path.display());
    fs::write(kubeconfig_path, decoded_bytes).context(error::WriteSnafu { what: "kubeconfig" })?;
    Ok(())
}

/// Extract the value of `RUST_LOG` if it exists, otherwise log this application at
/// `DEFAULT_AGENT_LEVEL_FILTER`.
pub fn init_agent_logger(bin_crate: &str, log_level: Option<LevelFilter>) {
    match env::var(env_logger::DEFAULT_FILTER_ENV).ok() {
        Some(_) => {
            // RUST_LOG exists; env_logger will use it.
            Builder::from_default_env().init();
        }
        None => {
            // RUST_LOG does not exist; use default log level except AWS SDK.
            let log_level = log_level.unwrap_or(DEFAULT_AGENT_LEVEL_FILTER);
            Builder::new()
                // Set log level to Error for crates other than our own.
                .filter_level(LevelFilter::Error)
                // Set all of our crates to the desired level.
                .filter(Some(bin_crate), log_level)
                .filter(Some("agent_common"), log_level)
                .filter(Some("bottlerocket_agents"), log_level)
                .filter(Some("model"), log_level)
                .filter(Some("resource_agent"), log_level)
                .filter(Some("test_agent"), log_level)
                .init();
        }
    }
}

/// Set up the config for aws calls using `aws_secret_name` if provided and `sts::assume_role`
/// if a role arn is provided.
pub async fn aws_test_config<R>(
    runner: &R,
    aws_secret_name: &Option<SecretName>,
    assume_role: &Option<String>,
    region: &Option<String>,
) -> Result<SdkConfig, R::E>
where
    R: Runner,
    <R as Runner>::E: From<Error>,
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
            RetryConfig::new()
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
                RetryConfig::new()
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
pub async fn setup_test_env<R>(runner: &R, aws_secret_name: &SecretName) -> Result<(), R::E>
where
    R: Runner,
    <R as Runner>::E: From<Error>,
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

    env::set_var("AWS_ACCESS_KEY_ID", access_key_id);
    env::set_var("AWS_SECRET_ACCESS_KEY", secret_access_key);

    Ok(())
}

/// Print a value using `serde_json` `to_string_pretty` for types that implement Serialize.
pub fn json_display<T: Serialize>(object: T) -> String {
    serde_json::to_string_pretty(&object).unwrap_or_else(|e| format!("Serialization failed: {}", e))
}

/// Implement `Display` using `serde_json` `to_string_pretty` for types that implement Serialize.
#[macro_export]
macro_rules! impl_display_as_json {
    ($i:ident) => {
        impl std::fmt::Display for $i {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let s = serde_json::to_string_pretty(self)
                    .unwrap_or_else(|e| format!("Serialization failed: {}", e));
                std::fmt::Display::fmt(&s, f)
            }
        }
    };
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
            RetryConfig::new()
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
                RetryConfig::new()
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

    env::set_var("AWS_ACCESS_KEY_ID", access_key_id);
    env::set_var("AWS_SECRET_ACCESS_KEY", secret_access_key);

    Ok(())
}

/// If the command was successful (exit code zero), returns the command's `stdout`. Otherwise
/// returns a provider error.
/// - `output`: the `Output` object from a `std::process::Command`
/// - `hint`: the command that was executed, e.g. `echo hello world`
/// - `resources`: whether or not resources will be leftover if this command failed
pub fn provider_error_for_cmd_output(
    output: Output,
    hint: &str,
    resources: Resources,
) -> ProviderResult<String> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.success() {
        Ok(stdout.to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let code = output.status.code().unwrap_or(-1);
        Err(ProviderError::new_with_context(
            resources,
            format!(
                "Error running '{}', exit code {}\nstderr:\n{}\nstdout:\n{}",
                hint, code, stderr, stdout
            ),
        ))
    }
}
