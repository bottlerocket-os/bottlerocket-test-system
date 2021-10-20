use std::fmt::{Display, Formatter};

/// When a [`Create`] or [`Destroy`] implementation returns an error, it must explicitly state
/// whether or not it has left resources behind.
#[derive(Copy, Clone, Debug)]
pub enum Resources {
    /// An error occurred, the provider was not able to clean up all resources, and these resources
    /// are orphaned in such a way that the provider can never destroy them. This is very bad and
    /// you should try to never return this!
    /// - The controller will **not** run `destroy`.    
    Orphaned,

    /// An error occurred, the provider was not able to clean up all resources. The provider
    /// may be able to destroy the resources if `Destroy::destroy` is called.
    /// - The controller **will** run `destroy`.
    Remaining,

    /// Something bad happened, but no resources were left behind.
    /// - The controller will **not** run `destroy`.    
    Clear,

    /// The provider does not know whether or not there are resources remaining.
    /// - The controller **will** run `destroy`.
    Unknown,
}

/// The error type returned by [`Create`] and [`Destroy`] implementations.
#[derive(Debug)]
pub struct ProviderError {
    /// Whether or not the error has left resources behind.
    resources: Resources,

    /// Any message to be included with the error. This will be included in the formatted display
    /// before `inner`.
    context: Option<String>,

    /// The error that caused this error.
    inner: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

/// The result type returned by [`Create`] and [`Destroy`] operations.
pub type ProviderResult<T> = std::result::Result<T, ProviderError>;

impl ProviderError {
    pub fn new_with_source_and_context<S, E>(resources: Resources, context: S, source: E) -> Self
    where
        S: Into<String>,
        E: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
    {
        Self {
            resources,
            context: Some(context.into()),
            inner: Some(source.into()),
        }
    }

    pub fn new_with_source<E>(resources: Resources, source: E) -> Self
    where
        E: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
    {
        Self {
            resources,
            context: None,
            inner: Some(source.into()),
        }
    }

    pub fn new_with_context<S>(resources: Resources, context: S) -> Self
    where
        S: Into<String>,
    {
        Self {
            resources,
            context: Some(context.into()),
            inner: None,
        }
    }

    pub fn resources(&self) -> Resources {
        self.resources
    }

    pub fn context(&self) -> Option<&str> {
        self.context.as_deref()
    }

    pub fn inner(&self) -> Option<&(dyn std::error::Error + Send + Sync + 'static)> {
        self.inner.as_ref().map(|some| some.as_ref())
    }
}

impl Display for ProviderError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.resources())?;
        if let Some(context) = self.context() {
            write!(f, ", {}", context)?;
        }
        if let Some(inner) = self.inner() {
            write!(f, ": {}", inner)?;
        }
        Ok(())
    }
}

impl Resources {
    pub fn message(&self) -> &'static str {
        match self {
            Resources::Orphaned => "An error left orphaned resources that cannot be destroyed",
            Resources::Remaining => "An error left resources behind that can be destroyed",
            Resources::Clear => "An error occurred but no resources were left behind",
            Resources::Unknown => {
                "An error occurred and it is unknown whether or not resources were left behind"
            }
        }
    }
}

impl Display for Resources {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self.message(), f)
    }
}

impl std::error::Error for ProviderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.inner()
            .map(|e| e as &(dyn std::error::Error + 'static))
    }
}
