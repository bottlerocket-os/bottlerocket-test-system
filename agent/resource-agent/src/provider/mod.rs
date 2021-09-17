mod error;

pub use self::error::{ProviderError, ProviderResult, Resources};
use crate::clients::InfoClient;
use model::Configuration;

/// Information needed by providers ([`Create`] and [`Destroy`] objects) during initialization.
#[derive(Debug, Clone)]
pub struct ProviderInfo<Config: Configuration> {
    /// Customizable configuration information for the provider.
    pub configuration: Config,
}

/// You implement the [`Create`] trait in order to create resources. This type is then injected into
/// the [`Agent`] object which drives the resource agent program in a Kubernetes-launched container.
///
/// There are four types that you may define to carry custom information.
///
/// ## Custom Types
///
/// - `Config` is the setup information needed for your `Create` implementation. This is provided
///   when the `Create` object is instantiated. For example, if your provided will default to a
///   certain region, this could be provided in the `Config`.
///
/// - `Info` is any data that you want to read and write to the Kubernetes CRD about your resource
///   creation or destruction process. For example, if you want to record a resource ID before you
///   have returned from the `create` function, you can do this with `Info`.
///
/// - `Request` is the information that users must provide in order for you to create resources. For
///   example, if you can create any number of instances, then the number of instances might be
///   provided in the `Request`.
///
/// - `Resource` is the information you provide back to user about the resource that you have
///    created.
///
#[async_trait::async_trait]
pub trait Create: Sized + Send + Sync {
    type Config: Configuration;
    type Info: Configuration;
    type Request: Configuration;
    type Resource: Configuration;

    /// Instantiate a new `Create` object.
    async fn new<I>(info: ProviderInfo<Self::Config>, client: &I) -> ProviderResult<Self>
    where
        I: InfoClient;

    /// Create resources as defined by the `request`. You may use `client` to record information
    /// with the Kubernetes CRD.
    async fn create<I>(&self, request: Self::Request, client: &I) -> ProviderResult<Self::Resource>
    where
        I: InfoClient;
}

/// You implement the [`Destroy`] trait in order to destroy resources that you have previously
/// created. This type is then injected into the [`Agent`] object which drives the resource agent
/// program in a Kubernetes-launched container.
///
/// There are three types that you may define to carry custom information.
///
/// ## Custom Types
///
/// - `Config` is the setup information needed for your `Destroy` implementation. This is provided
///   when the `Destroy` object is instantiated. For example, if your provided will default to a
///   certain region, this could be provided in the `Config`.
///
/// - `Info` is any data that you want to read and write to the Kubernetes CRD about your resource
///   creation or destruction process. For example, if you want to record the status of your
///   destruction process, you can do this with `Info`.
///
/// - `Resource` is the information you provided back to your user when you created the resource.
///
#[async_trait::async_trait]
pub trait Destroy: Sized {
    type Config: Configuration;
    type Info: Configuration;
    type Resource: Configuration;

    /// Instantiate a new `Destroy` object.
    async fn new<I>(info: ProviderInfo<Self::Config>, client: &I) -> ProviderResult<Self>
    where
        I: InfoClient;

    /// Destroy the resources. If a `Create` object returned an error or a Kubernetes call failed,
    /// then it might be possible that the `Resource` information cannot be obtained. If this
    /// happens, `destroy` will be called with `resource` as `None` and it is hoped that you can
    /// retrieve the necessary info with the `client` to clean up any resources that may exist.
    async fn destroy<I>(&self, resource: Option<Self::Resource>, client: &I) -> ProviderResult<()>
    where
        I: InfoClient;
}
