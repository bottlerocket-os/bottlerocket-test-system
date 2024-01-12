mod error;
mod job_builder;

pub(crate) use crate::job::error::{JobError, JobResult};
use aws_sdk_cloudwatchlogs::model::InputLogEvent;
pub(crate) use job_builder::{JobBuilder, JobType};
use k8s_openapi::api::batch::v1::Job;
use k8s_openapi::api::core::v1::Pod;
use k8s_openapi::chrono::{Duration, Utc};
use kube::api::{DeleteParams, ListParams, LogParams, PropagationPolicy};
use kube::{Api, ResourceExt};
use log::{debug, info, warn};
use snafu::{ensure, OptionExt, ResultExt};
use std::env;
use std::time::{SystemTime, UNIX_EPOCH};
use testsys_model::constants::NAMESPACE;
use testsys_model::system::TESTSYS_CONTROLLER_ARCHIVE_LOGS;

lazy_static::lazy_static! {
    /// The maximum amount of time for a test to begin running (in seconds).
    pub static ref TEST_START_TIME_LIMIT: Duration = {
        Duration::seconds(30)
    };
}

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
    Running(Option<Duration>),
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
        error::TooManyJobContainersSnafu {
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
        let job_running_duration = status
            .start_time
            .as_ref()
            .map(|start_time| Utc::now() - start_time.0);
        Ok(JobState::Running(job_running_duration))
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
                propagation_policy: Some(PropagationPolicy::Background),
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

async fn get_pod(k8s_client: kube::Client, job_name: &str) -> JobResult<String> {
    let pod_api: Api<Pod> = Api::namespaced(k8s_client, NAMESPACE);
    let name = pod_api
        .list(&ListParams {
            label_selector: Some(format!("job-name={}", job_name)),
            ..Default::default()
        })
        .await
        .context(error::NotFoundSnafu {})?
        .items
        .first()
        .context(error::NoPodsSnafu {
            job: job_name.to_string(),
        })?
        .name_any();

    Ok(name)
}

async fn pod_logs(k8s_client: kube::Client, pod_name: &str) -> JobResult<String> {
    let log_params = LogParams {
        follow: false,
        pretty: true,
        ..Default::default()
    };
    let pod_api: Api<Pod> = Api::namespaced(k8s_client, NAMESPACE);

    pod_api
        .logs(pod_name, &log_params)
        .await
        .context(error::NoLogsSnafu { pod: pod_name })
}

pub(crate) async fn archive_logs(k8s_client: kube::Client, job_name: &str) -> JobResult<()> {
    let archive_logs = match env::var(TESTSYS_CONTROLLER_ARCHIVE_LOGS) {
        Ok(s) => s == true.to_string(),
        Err(e) => {
            warn!(
                "Unable to read environment variable '{}': {}",
                TESTSYS_CONTROLLER_ARCHIVE_LOGS, e
            );
            false
        }
    };

    if !archive_logs {
        return Ok(());
    }
    let config = aws_config::from_env().load().await;
    let client = aws_sdk_cloudwatchlogs::Client::new(&config);

    match client
        .create_log_group()
        .log_group_name("testsys")
        .send()
        .await
    {
        Ok(_) => info!("Creating log group"),
        Err(e) => {
            let service_error = e.into_service_error();
            if service_error.is_resource_already_exists_exception() {
                info!("Log group already exists.")
            }
            return Err(error::JobError::CreateLogGroup {
                message: service_error.to_string(),
                log_group: "testsys".to_string(),
            });
        }
    }

    let pod_name = get_pod(k8s_client.clone(), job_name).await?;
    let logs = pod_logs(k8s_client, &pod_name).await?;
    let name = format!(
        "{}-{}",
        job_name,
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()
    );

    client
        .create_log_stream()
        .log_group_name("testsys")
        .log_stream_name(&name)
        .send()
        .await
        .context(error::CreateLogStreamSnafu {
            log_stream: name.to_string(),
        })?;

    client
        .put_log_events()
        .log_group_name("testsys")
        .log_stream_name(&name)
        .log_events(
            InputLogEvent::builder()
                .message(logs)
                .timestamp(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)?
                        .as_millis()
                        .try_into()
                        .unwrap_or_default(),
                )
                .build(),
        )
        .send()
        .await
        .context(error::CreateLogEventSnafu { log_event: &name })?;

    info!("Archive of '{job_name}' can be found at '{name}'");

    Ok(())
}
