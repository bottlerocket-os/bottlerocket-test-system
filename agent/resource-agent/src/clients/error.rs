use crate::error::{ErrorEnum, ErrorMessage};
use std::error::Error;
use std::fmt::{Display, Formatter};

/// The result type returned by [`InfoClient`] and [`AgentClient`] implementations.
pub type ClientResult<T> = std::result::Result<T, ClientError>;

/// The error type returned by [`InfoClient`] and [`AgentClient`] implementations.
#[derive(Debug)]
pub enum ClientError {
    /// The client could not be created.
    InitializationFailed(Option<Box<dyn std::error::Error + Send + Sync + 'static>>),

    /// Some data that was expected to be present was not found.
    MissingData(Option<ErrorMessage>),

    /// A communication with Kubernetes failed.
    RequestFailed(Option<Box<dyn std::error::Error + Send + Sync + 'static>>),

    /// An error occurred serializing or deserializing.
    Serialization(Option<Box<dyn std::error::Error + Send + Sync + 'static>>),

    /// An error occurred while reading a secrets file.
    SecretsError(Option<Box<dyn std::error::Error + Send + Sync + 'static>>),
}

impl ErrorEnum for ClientError {
    fn variant_name(&self) -> &'static str {
        match self {
            ClientError::InitializationFailed(_) => "Initialization failed",
            ClientError::MissingData(_) => "Missing data",
            ClientError::RequestFailed(_) => "Request failed",
            ClientError::Serialization(_) => "Serialization error",
            ClientError::SecretsError(_) => "Secrets error",
        }
    }

    fn inner(&self) -> Option<&(dyn std::error::Error + Send + Sync + 'static)> {
        match self {
            ClientError::InitializationFailed(e) => e.as_ref().map(|some| some.as_ref()),
            ClientError::MissingData(s) => s
                .as_ref()
                .map(|some| some as &(dyn std::error::Error + Send + Sync + 'static)),
            ClientError::RequestFailed(e) => e.as_ref().map(|some| some.as_ref()),
            ClientError::Serialization(e) => e.as_ref().map(|some| some.as_ref()),
            ClientError::SecretsError(e) => e.as_ref().map(|some| some.as_ref()),
        }
    }
}

impl Error for ClientError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.inner_as_source()
    }
}

impl Display for ClientError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.display(f)
    }
}
