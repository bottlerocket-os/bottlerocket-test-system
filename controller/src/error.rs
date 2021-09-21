use std::fmt::{Debug, Display, Formatter};

pub(crate) type Error = anyhow::Error;
pub(crate) type Result<T> = std::result::Result<T, Error>;

/// `anyhow::Error` does not implement `std::error::Error` but `kube-rs` wants this, so we create
/// a thin wrapper.
pub(crate) struct ReconciliationError(anyhow::Error);
pub(crate) type ReconciliationResult<T> = std::result::Result<T, ReconciliationError>;

impl Display for ReconciliationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl Debug for ReconciliationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

impl std::error::Error for ReconciliationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

impl From<anyhow::Error> for ReconciliationError {
    fn from(e: anyhow::Error) -> Self {
        ReconciliationError(e)
    }
}
