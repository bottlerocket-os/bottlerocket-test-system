/*!

The `bootstrap` module defines a struct and function for getting the necessary information from the
container environment to construct the [`Agent`] and all of its parts.

!*/
use crate::ResourceAction;
use model::constants::{ENV_RESOURCE_ACTION, ENV_RESOURCE_NAME};
use snafu::{ResultExt, Snafu};
use std::str::FromStr;

/// The public error type for the default [`Bootstrap`].
#[derive(Debug, Snafu)]
pub struct BootstrapError(InnerError);

/// The private error type for the default [`Bootstrap`].
#[derive(Debug, Snafu)]
pub(crate) enum InnerError {
    #[snafu(display("Unable to parse '{}' as an action", bad_value))]
    BadAction { bad_value: String },

    #[snafu(display("Unable to read environment variable: '{}': {}", key, source))]
    EnvRead {
        key: String,
        source: std::env::VarError,
    },

    #[snafu(display("Incorrect resource action '{}': {}", value, source))]
    ResourceActionParse { value: String, source: model::Error },
}

/// Data that is read from the TestPod's container environment and filesystem.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BootstrapData {
    /// The unique name of the resource that we are providing.
    pub resource_name: String,
    /// The action that we should take.
    pub action: ResourceAction,
}

impl BootstrapData {
    pub fn from_env() -> Result<BootstrapData, BootstrapError> {
        let action_str = std::env::var(ENV_RESOURCE_ACTION).context(EnvRead {
            key: ENV_RESOURCE_ACTION,
        })?;
        let action = ResourceAction::from_str(&action_str)
            .context(ResourceActionParse { value: action_str })?;
        Ok(BootstrapData {
            resource_name: std::env::var(ENV_RESOURCE_NAME).context(EnvRead {
                key: ENV_RESOURCE_NAME,
            })?,
            action,
        })
    }
}
