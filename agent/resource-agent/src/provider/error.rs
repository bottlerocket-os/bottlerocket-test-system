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

/// This is a trait that you can implement for your `Info` type to describe whether or not resources
/// remain that need to be cleaned up.
///
/// # Example
///
/// ```
/// use resource_agent::provider::{AsResources, ProviderError, IntoProviderError, Resources};
/// struct MyInfoType {
///     ids: Vec<usize>
/// }
/// impl AsResources for MyInfoType {
///     fn as_resources(&self) -> Resources {
///         if self.ids.is_empty() {
///             Resources::Clear
///         } else {
///             Resources::Remaining
///         }
///     }
/// }
/// ```
///
pub trait AsResources {
    /// Inspects `&self` and determines if there are resources remaining.
    fn as_resources(&self) -> Resources;
}

// Implement the trivial case of `AsResources` for the `Resources` enum itself.
impl AsResources for Resources {
    fn as_resources(&self) -> Resources {
        *self
    }
}

// Implement the trivial case of `AsResources` for a ref to the `Resources` enum itself.
impl AsResources for &Resources {
    fn as_resources(&self) -> Resources {
        **self
    }
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
    pub fn new_with_source_and_context<R, S, E>(resources: R, context: S, source: E) -> Self
    where
        R: AsResources,
        S: Into<String>,
        E: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
    {
        Self {
            resources: resources.as_resources(),
            context: Some(context.into()),
            inner: Some(source.into()),
        }
    }

    pub fn new_with_source<R, E>(resources: R, source: E) -> Self
    where
        R: AsResources,
        E: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
    {
        Self {
            resources: resources.as_resources(),
            context: None,
            inner: Some(source.into()),
        }
    }

    pub fn new_with_context<R, S>(resources: R, context: S) -> Self
    where
        R: AsResources,
        S: Into<String>,
    {
        Self {
            resources: resources.as_resources(),
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
            write!(f, ": {:?}", inner)?;
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

// Make `ProviderError` function as a standard error.
impl std::error::Error for ProviderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.inner()
            .map(|e| e as &(dyn std::error::Error + 'static))
    }
}

/// A trait that makes it possible to convert error types to `ProviderError` using a familiar
/// `context` function.
pub trait IntoProviderError<T> {
    /// Convert `self` into a `ProviderError`.
    fn context<R, S>(self, resources: R, message: S) -> ProviderResult<T>
    where
        S: Into<String>,
        R: AsResources;
}

// Implement `IntoProviderError` for all standard `Error + Send + Sync + 'static` types.
impl<T, E> IntoProviderError<T> for std::result::Result<T, E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn context<R, S>(self, resources: R, message: S) -> ProviderResult<T>
    where
        S: Into<String>,
        R: AsResources,
    {
        self.map_err(|e| ProviderError::new_with_source_and_context(resources, message, e))
    }
}

// Implement `IntoProviderError` for options where `None` is converted into an error.
impl<T> IntoProviderError<T> for std::option::Option<T> {
    fn context<R, S>(self, r: R, m: S) -> Result<T, ProviderError>
    where
        S: Into<String>,
        R: AsResources,
    {
        self.ok_or_else(|| ProviderError::new_with_context(r, m))
    }
}
