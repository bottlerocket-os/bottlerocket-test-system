use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub(crate) struct Error {
    /// Any message to be included with the error. This will be included in the formatted display
    /// before `inner`.
    context: Option<String>,

    /// The error that caused this error.
    inner: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

pub(crate) type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn new_with_source_and_context<S, E>(context: S, source: E) -> Self
    where
        S: Into<String>,
        E: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
    {
        Self {
            context: Some(context.into()),
            inner: Some(source.into()),
        }
    }

    pub fn new_with_context<S>(context: S) -> Self
    where
        S: Into<String>,
    {
        Self {
            context: Some(context.into()),
            inner: None,
        }
    }

    pub fn context(&self) -> Option<&str> {
        self.context.as_deref()
    }

    pub fn inner(&self) -> Option<&(dyn std::error::Error + Send + Sync + 'static)> {
        self.inner.as_ref().map(|some| some.as_ref())
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Some(context) = self.context() {
            write!(f, ", {}", context)?;
        }
        if let Some(inner) = self.inner() {
            write!(f, ": {}", inner)?;
        }
        Ok(())
    }
}

// Make `Error` function as a standard error.
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.inner()
            .map(|e| e as &(dyn std::error::Error + 'static))
    }
}

/// A trait that makes it possible to convert error types to `Error` using a familiar
/// `context` function.
pub(crate) trait IntoError<T> {
    /// Convert `self` into a `Error`.
    fn context<S>(self, message: S) -> Result<T>
    where
        S: Into<String>;
}

// Implement `IntoError` for all standard `Error + Send + Sync + 'static` types.
impl<T, E> IntoError<T> for std::result::Result<T, E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn context<S>(self, message: S) -> Result<T>
    where
        S: Into<String>,
    {
        self.map_err(|e| Error::new_with_source_and_context(message, e))
    }
}

// Implement `IntoError` for options where `None` is converted into an error.
impl<T> IntoError<T> for std::option::Option<T> {
    fn context<S>(self, m: S) -> Result<T>
    where
        S: Into<String>,
    {
        self.ok_or_else(|| Error::new_with_context(m))
    }
}
