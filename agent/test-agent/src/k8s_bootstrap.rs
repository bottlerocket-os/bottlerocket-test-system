use crate::{Bootstrap, BootstrapData, DefaultBootstrap};
use async_trait::async_trait;
use model::model::ENV_TEST_NAME;
use snafu::{ResultExt, Snafu};
use std::fmt::Debug;

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

#[async_trait]
impl Bootstrap for DefaultBootstrap {
    type E = BootstrapError;

    async fn read(&self) -> Result<BootstrapData, Self::E> {
        Ok(BootstrapData {
            test_name: std::env::var(ENV_TEST_NAME).context(EnvRead { key: ENV_TEST_NAME })?,
        })
    }
}
