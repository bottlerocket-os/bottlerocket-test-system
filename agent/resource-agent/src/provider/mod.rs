mod error;

pub use self::error::{AsResources, IntoProviderError, ProviderError, ProviderResult, Resources};
use crate::clients::InfoClient;
use serde::Serialize;
use std::collections::BTreeMap;
use testsys_model::{Configuration, SecretName, SecretType};

#[derive(Debug, Default, Clone, Serialize)]
pub struct Spec<C>
where
    C: Configuration,
{
    pub configuration: C,
    pub secrets: BTreeMap<SecretType, SecretName>,
}

/// You implement the [`Create`] trait in order to create resources. This type is then injected into
/// the [`Agent`] object which drives the resource agent program in a Kubernetes-launched container.
///
/// There are four types that you may define to carry custom information.
///
/// ## Custom Types
///
/// - `Config` is the information that users must provide in order for you to create resources. For
///   example, if you can create any number of instances, then the number of instances might be
///   provided in the `Config`.
///
/// - `Info` is any data that you want to read and write to the Kubernetes CRD about your resource
///   creation or destruction process. For example, if you want to record a resource ID before you
///   have returned from the `create` function, you can do this with `Info`.
///
/// - `Resource` is the information you provide back to user about the resource that you have
///    created.
///
#[async_trait::async_trait]
pub trait Create: Sized + Send + Sync {
    type Config: Configuration;
    type Info: Configuration;
    type Resource: Configuration;

    /// Create resources as defined by the `spec`. You may use `client` to record information
    /// with the Kubernetes CRD.
    async fn create<I>(
        &self,
        spec: Spec<Self::Config>,
        client: &I,
    ) -> ProviderResult<Self::Resource>
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
/// - `Config` is the information that users must provide in order for the [`Create`] trait to
///   create resources. For example, if it can create any number of instances, then the number of
///   instances might be provided in the `Config`. It is provided, when possible, to the `Destroy`
///   trait as well.
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

    /// Destroy the resources. If a `Create` object returned an error or a Kubernetes call failed,
    /// then it might be possible that the `Resource` information cannot be obtained. If this
    /// happens, `destroy` will be called with `resource` as `None` and it is hoped that you can
    /// retrieve the necessary info with the `client` to clean up any resources that may exist.
    async fn destroy<I>(
        &self,
        spec: Option<Spec<Self::Config>>,
        resource: Option<Self::Resource>,
        client: &I,
    ) -> ProviderResult<()>
    where
        I: InfoClient;
}
