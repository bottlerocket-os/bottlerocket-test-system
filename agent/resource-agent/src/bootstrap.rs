/*!

The `bootstrap` module defines a struct and function for getting the necessary information from the
container environment to construct the [`Agent`] and all of its parts.

!*/
use client::model::{
    ENV_RESOURCE_ACTION, ENV_RESOURCE_NAME, ENV_RESOURCE_PROVIDER_NAME, ENV_TEST_NAME,
};
use snafu::{ResultExt, Snafu};
use std::convert::TryFrom;
use std::fmt::{Debug, Display, Formatter};

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
}

/// When the controller runs a resource agent, it will tell it whether it should create or destroy
/// resources (like a sub-command).
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Action {
    /// Create resources.
    Create,
    /// Destroy resources.
    Destroy,
}

/// Data that is read from the TestPod's container environment and filesystem.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BootstrapData {
    /// The name of the TestSys Test.
    pub test_name: String,
    /// The name of this resource provider.
    pub resource_provider_name: String,
    /// The unique name of the resource that we are providing.
    pub resource_name: String,
    /// The action that we should take.
    pub action: Action,
}

impl BootstrapData {
    pub fn from_env() -> Result<BootstrapData, BootstrapError> {
        Ok(BootstrapData {
            test_name: std::env::var(ENV_TEST_NAME).context(EnvRead { key: ENV_TEST_NAME })?,
            resource_name: std::env::var(ENV_RESOURCE_NAME).context(EnvRead {
                key: ENV_RESOURCE_NAME,
            })?,
            resource_provider_name: std::env::var(ENV_RESOURCE_PROVIDER_NAME).context(EnvRead {
                key: ENV_RESOURCE_PROVIDER_NAME,
            })?,
            action: Action::try_from(
                std::env::var(ENV_RESOURCE_ACTION)
                    .context(EnvRead {
                        key: ENV_RESOURCE_ACTION,
                    })?
                    .as_str(),
            )?,
        })
    }
}

impl TryFrom<&str> for Action {
    type Error = BootstrapError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.as_ref() {
            "create" => Ok(Self::Create),
            "destroy" => Ok(Self::Destroy),
            bad_value => Err(BootstrapError(BadAction { bad_value }.build())),
        }
    }
}

impl Display for Action {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Action::Create => Display::fmt("create", f),
            Action::Destroy => Display::fmt("destroy", f),
        }
    }
}
