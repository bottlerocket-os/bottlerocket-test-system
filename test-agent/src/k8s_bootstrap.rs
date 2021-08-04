use crate::{Bootstrap, BootstrapData, DefaultBootstrap};
use async_trait::async_trait;
use snafu::Snafu;
use std::fmt::Debug;

/// The public error type for the default [`Bootstrap`].
#[derive(Debug, Snafu)]
pub struct BootstrapError(InnerError);

/// The private error type for the default [`Bootstrap`].
#[derive(Debug, Snafu)]
pub(crate) enum InnerError {}

#[async_trait]
impl Bootstrap for DefaultBootstrap {
    type E = BootstrapError;

    async fn read(&self) -> Result<BootstrapData, Self::E> {
        Ok(BootstrapData {
            // TODO - read from the container environment or filesystem
            test_name: "hello-world".to_string(),
        })
    }
}
