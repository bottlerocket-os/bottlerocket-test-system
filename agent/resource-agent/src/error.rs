use crate::bootstrap::BootstrapError;
use crate::clients::ClientError;
use crate::provider::ProviderError;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};

/// This is the error type returned by an [`Agent`] object. When receiving this error type you will
/// typically want to display it and exit your main function with a non-zero exit code.
#[derive(Debug)]
pub enum AgentError {
    Bootstrap(BootstrapError),
    Client(ClientError),
    Provider(ProviderError),
}

/// The result type returned by an [`Agent`] object.
pub type AgentResult<T> = std::result::Result<T, AgentError>;

impl ErrorEnum for AgentError {
    fn variant_name(&self) -> &'static str {
        match self {
            AgentError::Bootstrap(_) => "Bootstrap error",
            AgentError::Client(_) => "Client error",
            AgentError::Provider(_) => "Provider error",
        }
    }

    fn inner(&self) -> Option<&(dyn Error + Send + Sync + 'static)> {
        match self {
            AgentError::Bootstrap(e) => Some(e as &(dyn Error + Send + Sync + 'static)),
            AgentError::Client(e) => Some(e as &(dyn Error + Send + Sync + 'static)),
            AgentError::Provider(e) => Some(e as &(dyn Error + Send + Sync + 'static)),
        }
    }
}

impl Display for AgentError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.display(f)
    }
}

impl Error for AgentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.inner_as_source()
    }
}

impl From<BootstrapError> for AgentError {
    fn from(e: BootstrapError) -> Self {
        Self::Bootstrap(e)
    }
}

impl From<ClientError> for AgentError {
    fn from(e: ClientError) -> Self {
        Self::Client(e)
    }
}

impl From<ProviderError> for AgentError {
    fn from(e: ProviderError) -> Self {
        Self::Provider(e)
    }
}

/// This struct can serve as an `Error` type when you want to provide an error message, but have no
/// underlying error type. It allows a string to serve as an error. This can be useful for custom
/// (i.e. mock) implementations of the [`InfoClient`] and [`AgentClient`].
///
/// # Example
///
/// ```
/// # use resource_agent::error::ErrorMessage;
/// // Create a std::error::Error from a string.
/// let _error: ErrorMessage = "Something bad happened".into();
/// ```
///
#[derive(Debug)]
pub struct ErrorMessage {
    message: String,
}

impl Display for ErrorMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.message, f)
    }
}

impl<S: Into<String>> From<S> for ErrorMessage {
    fn from(s: S) -> Self {
        Self { message: s.into() }
    }
}

impl std::error::Error for ErrorMessage {}

/// This internal trait helps de-duplicate a bit of code that we use when implementing `Display`
/// and `Error` for our error enums.
pub(crate) trait ErrorEnum {
    fn variant_name(&self) -> &'static str;
    fn inner(&self) -> Option<&(dyn std::error::Error + Send + Sync + 'static)>;

    fn display(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.inner() {
            None => Display::fmt(self.variant_name(), f),
            Some(inner) => write!(f, "{}: {}", self.variant_name(), inner),
        }
    }

    fn inner_as_source(&self) -> Option<&(dyn Error + 'static)> {
        self.inner().map(|some| some as &(dyn Error + 'static))
    }
}
