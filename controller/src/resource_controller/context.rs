use crate::error::Result;
use crate::job::{delete_job, get_job_state, JobBuilder, JobState, JobType};
use anyhow::Context as AnyhowContext;
use kube::Api;
use log::debug;
use model::clients::{CrdClient, ResourceClient};
use model::constants::{ENV_RESOURCE_ACTION, ENV_RESOURCE_NAME};
use model::{CrdExt, Resource, ResourceAction};
use std::sync::Arc;

/// This is used by `kube-runtime` to pass any custom information we need when [`reconcile`] is
/// called.
pub(super) type Context = Arc<ContextData>;

pub(super) fn new_context(client: kube::Client) -> Context {
    Arc::new(ContextData {
        resource_client: ResourceClient::new_from_k8s_client(client),
    })
}

/// This type is wrapped by [`kube::Context`] and contains information we need during [`reconcile`].
#[derive(Clone)]
pub(crate) struct ContextData {
    resource_client: ResourceClient,
}

impl ContextData {
    pub(super) fn api(&self) -> &Api<Resource> {
        self.resource_client.api()
    }
}

/// The [`reconcile`] function has [`Resource`] and [`Context`] as its inputs. For convenience, we
/// combine these and provide accessor and helper functions.
pub(super) struct ResourceInterface {
    resource: Resource,
    context: Context,
    creation_job: String,
    destruction_job: String,
}

impl ResourceInterface {
    pub(super) fn new(resource: Resource, context: Context) -> Result<Self> {
        let creation_job = format!("{}-creation", resource.object_name());
        let destruction_job = format!("{}-destruction", resource.object_name());
        Ok(Self {
            resource,
            context,
            creation_job,
            destruction_job,
        })
    }

    pub(super) fn name(&self) -> &str {
        self.resource().object_name()
    }

    pub(super) fn resource(&self) -> &Resource {
        &self.resource
    }

    pub(super) fn api(&self) -> &Api<Resource> {
        self.context.api()
    }

    pub(super) fn resource_client(&self) -> &ResourceClient {
        &self.context.resource_client
    }

    pub(super) fn k8s_client(&self) -> kube::Client {
        self.api().clone().into_client()
    }

    pub(super) async fn get_job_state(&self, op: ResourceAction) -> Result<JobState> {
        self.get_job_state_by_name(self.job_name(op)).await
    }

    pub(super) async fn start_job(&self, op: ResourceAction) -> Result<()> {
        let job_name = self.job_name(op);
        let deploy_result = JobBuilder {
            agent: &self.resource().spec.agent,
            job_name,
            job_type: JobType::ResourceAgent,
            environment_variables: vec![
                (ENV_RESOURCE_ACTION, op.to_string()),
                (ENV_RESOURCE_NAME, self.name().to_owned()),
            ],
        }
        .deploy(self.resource_client().api().clone().into_client())
        .await;

        if let Err(crate::job::JobError::AlreadyExists { .. }) = &deploy_result {
            debug!(
                "We tried to create the job '{}' but it already existed",
                job_name
            );
            return Ok(());
        }
        let _ = deploy_result.with_context(|| format!("Unable to deploy job '{}'", job_name))?;
        Ok(())
    }

    pub(super) async fn remove_job(&self, op: ResourceAction) -> Result<()> {
        delete_job(self.k8s_client(), self.job_name(op))
            .await
            .context(format!("Unable to remove job '{}'", self.job_name(op)))?;
        Ok(())
    }

    async fn get_job_state_by_name(&self, job_name: &str) -> Result<JobState> {
        get_job_state(self.k8s_client(), job_name)
            .await
            .context(format!("Unable to get state of job '{}'", job_name))
    }

    fn job_name(&self, op: ResourceAction) -> &str {
        match op {
            ResourceAction::Create => &self.creation_job,
            ResourceAction::Destroy => &self.destruction_job,
        }
    }
}
