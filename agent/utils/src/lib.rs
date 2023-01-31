/*!

`agent-utils` is a collection of functions that may be used by agent implementations.
`aws` contains several functions that can be used to set up an aws environment.

!*/

use constants::DEFAULT_AGENT_LEVEL_FILTER;
use env_logger::Builder;
pub use error::Error;
use log::LevelFilter;
use resource_agent::provider::{ProviderError, ProviderResult, Resources};
use serde::Serialize;
use snafu::ResultExt;
use std::path::Path;
use std::process::Output;
use std::{env, fs};

pub mod aws;
pub mod constants;
mod error;

/// Decode base64 blob and write to a file at the specified path
pub async fn base64_decode_write_file(
    base64_content: &str,
    path_to_write_to: &str,
) -> Result<(), error::Error> {
    let path = Path::new(path_to_write_to);
    let decoded_bytes =
        base64::decode(base64_content.as_bytes()).context(error::Base64DecodeSnafu)?;
    fs::write(path, decoded_bytes).context(error::WriteFileSnafu {
        path: path_to_write_to,
    })?;
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
                .filter(Some("resource_agent"), log_level)
                .filter(Some("test_agent"), log_level)
                .filter(Some("testsys_model"), log_level)
                .init();
        }
    }
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
