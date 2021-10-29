mod error;
mod job_builder;

pub(crate) use crate::job::error::{JobError, JobResult};
pub(crate) use job_builder::{JobBuilder, JobType};
use k8s_openapi::api::batch::v1::Job;
use kube::api::{DeleteParams, PropagationPolicy};
use kube::Api;
use log::debug;
use model::constants::NAMESPACE;
use snafu::ensure;

/// We run the test pod using a k8s `Job`. Jobs can run many containers and provide counts of how
/// many containers are running or have completed (succeeded or failed). We are only running one
/// container, so it is helpful to transform those counts into a simple enumeration of our job's
/// state.
#[derive(Debug, Copy, Clone)]
pub(crate) enum JobState {
    /// The job does not exist.
    None,
    /// The job exists but we cannot figure out the status of its container. Hopefully this is
    /// transient and you can check the job again later.
    Unknown,
    /// The job is running.
    Running,
    /// The job is no longer running, and the container exited with a failure code.
    Failed,
    /// The job is no longer running, and the container exited with `0`. We avoid calling this
    /// 'success' because the agent may have reported an error to the CRD.
    Exited,
}

pub(crate) async fn get_job_state<S>(k8s_client: kube::Client, name: S) -> JobResult<JobState>
where
    S: AsRef<str>,
{
    let api: Api<Job> = Api::namespaced(k8s_client, NAMESPACE);
    let result = api.get(name.as_ref()).await.map_err(JobError::get);
    if let Err(JobError::NotFound { .. }) = &result {
        Ok(JobState::None)
    } else {
        let job = result?;
        parse_job_state(&job)
    }
}

/// Transform the container counts in `job.status` to a `JobState`
fn parse_job_state(job: &Job) -> JobResult<JobState> {
    // Return early if `job.status` is somehow `None`.
    let status = match &job.status {
        None => {
            return Ok(JobState::Unknown);
        }
        Some(some) => some,
    };

    // Unwrap the container counts defaulting to zero if they are missing.
    let running = status.active.unwrap_or(0);
    let succeeded = status.succeeded.unwrap_or(0);
    let failed = status.failed.unwrap_or(0);

    // Return early if there are no containers counted. It probably means the container hasn't
    // started yet.
    if running + succeeded + failed == 0 {
        return Ok(JobState::Unknown);
    }

    // There should be exactly one container.
    ensure!(
        running + succeeded + failed == 1,
        error::TooManyJobContainers {
            job_name: job
                .metadata
                .name
                .as_ref()
                .map_or("unknown", |name| name.as_str()),
            running,
            succeeded,
            failed
        }
    );

    if running == 1 {
        Ok(JobState::Running)
    } else if succeeded == 1 {
        Ok(JobState::Exited)
    } else {
        Ok(JobState::Failed)
    }
}

pub(crate) async fn delete_job(k8s_client: kube::Client, name: &str) -> JobResult<()> {
    let api: Api<Job> = Api::namespaced(k8s_client, NAMESPACE);
    let result = api
        .delete(
            name,
            &DeleteParams {
                dry_run: false,
                grace_period_seconds: Some(0),
                propagation_policy: Some(PropagationPolicy::Foreground),
                preconditions: None,
            },
        )
        .await
        .map_err(JobError::delete);
    if matches!(result, Err(JobError::NotFound { .. })) {
        debug!("We tried to delete the job '{}' but it did not exist", name);
        return Ok(());
    }
    let _ = result?;
    Ok(())
}
