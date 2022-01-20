/*!

The `bootstrap` module defines a struct and function for getting the necessary information from the
container environment to construct the [`Agent`] and all of its parts.

!*/

use model::constants::ENV_TEST_NAME;
use snafu::{ResultExt, Snafu};

/// Data that is read from the TestPod's container environment and filesystem.
pub struct BootstrapData {
    /// The name of the TestSys Test.
    pub test_name: String,
}

/// The public error type for the default [`Bootstrap`].
#[derive(Debug, Snafu)]
pub struct BootstrapError(InnerError);

/// The private error type for the default [`Bootstrap`].
#[derive(Debug, Snafu)]
pub(crate) enum InnerError {
    #[snafu(display("Unable to read environment variable: '{}': {}", key, source))]
    EnvRead {
        key: String,
        source: std::env::VarError,
    },
}

impl BootstrapData {
    pub fn from_env() -> Result<BootstrapData, BootstrapError> {
        Ok(BootstrapData {
            test_name: std::env::var(ENV_TEST_NAME).context(EnvReadSnafu { key: ENV_TEST_NAME })?,
        })
    }
}
