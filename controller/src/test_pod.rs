use crate::context::TestInterface;
use crate::error::{self, Result};
use crate::reconcile::REQUEUE;
use k8s_openapi::api::batch::v1::{Job, JobSpec};
use k8s_openapi::api::core::v1::{
    Container, EnvVar, LocalObjectReference, PodSpec, PodTemplateSpec,
};
use kube::api::{DeleteParams, ListParams, ObjectMeta, PostParams, PropagationPolicy};
use kube_runtime::controller::ReconcilerAction;
use log::{debug, error, trace};
use model::constants::{
    APP_COMPONENT, APP_CREATED_BY, APP_INSTANCE, APP_MANAGED_BY, APP_NAME, APP_PART_OF, CONTROLLER,
    ENV_TEST_NAME, LABEL_TEST_NAME, LABEL_TEST_UID, NAMESPACE, TESTSYS, TEST_AGENT,
    TEST_AGENT_SERVICE_ACCOUNT,
};
use model::{Lifecycle, RunState};
use snafu::{ensure, ResultExt};
use std::collections::BTreeMap;

/// Runs a k8s `Job` to run our test pod. Adds the pod finalizer to ensure we don't forget to clean
/// up the `Job` later.
///
/// # Preconditions
///
/// Assumes that the pod finalizer is not present. If it is, A duplicate finalizer error will occur.
///
pub(crate) async fn create_test_pod(test: &mut TestInterface) -> Result<ReconcilerAction> {
    debug!("Creating test pod for '{}'", test.name());
    test.add_pod_finalizer().await?;
    if let Err(create_job_error) = create_job(test).await {
        if let Err(remove_finalizer_error) = test.remove_pod_finalizer().await {
            error!(
                "Unable to remove pod finalizer after inability to create test pod for test '{}': {}",
                test.name(),
                remove_finalizer_error
            );
        }
        return Err(create_job_error);
    }
    let mut status = test.controller_status().into_owned();
    status.lifecycle = Lifecycle::TestPodCreated;
    test.set_controller_status(status).await?;
    Ok(REQUEUE)
}

/// Deletes the k8s `Job` if the pod finalizer is present. See `delete_job` for more.
pub(crate) async fn delete_test_pod(test: &mut TestInterface) -> Result<ReconcilerAction> {
    if !test.has_pod_finalizer() {
        return Ok(REQUEUE);
    }
    debug!("Deleting test pod for '{}'", test.name());
    delete_job(test).await?;
    Ok(REQUEUE)
}

/// Checks the status of the test pod's `Job`. Updates the TestSys `Test`'s `Lifecycle` state
/// accordingly.
///
/// # Preconditions
///
/// Assumes the k8s `Job` is present. Will error otherwise due to k8s returning 'not found'.
///
pub(crate) async fn check_test_pod(test: &mut TestInterface) -> Result<ReconcilerAction> {
    trace!("Checking test pod for '{}'", test.name());
    let mut status = test.controller_status().into_owned();
    // TODO - enforce a timeout/deadline for pods that never start
    // https://github.com/bottlerocket-os/bottlerocket-test-system/issues/76
    let job_api = test.job_api();
    let job = job_api.get(test.name()).await.context(error::KubeClient {
        action: "get job",
        test_name: test.name(),
    })?;

    let job_state = JobState::from_job(&job)?;

    match job_state {
        JobState::Unknown => {
            trace!(
                "Job for test '{}' is created but pod not found yet.",
                test.name()
            );
            status.lifecycle = Lifecycle::TestPodStarting;
        }
        JobState::Running => {
            trace!(
                "Pod for test '{}' is created and has not completed.",
                test.name()
            );
            match test.agent_status().run_state {
                RunState::Running => status.lifecycle = Lifecycle::TestPodHealthy,
                RunState::Done => status.lifecycle = Lifecycle::TestPodDone,
                RunState::Error => status.lifecycle = Lifecycle::TestPodError,
                RunState::Unknown => { /* Waiting for agent to start. No status change. */ }
            }
        }
        JobState::Failed => {
            trace!("Pod for test '{}' failed.", test.name());
            status.lifecycle = Lifecycle::TestPodFailed;
            // TODO - find out why and add an error message to the test
        }
        JobState::Succeeded => {
            trace!("Pod for test '{}' exited zero.", test.name());
            status.lifecycle = Lifecycle::TestPodExited;
        }
    }
    test.set_controller_status(status).await?;
    Ok(REQUEUE)
}

/// Get the TestSys `Test`'s k8s `Job` if present. Returns `None` if the `Job` does not exist.
pub(crate) async fn get_job(test: &mut TestInterface) -> Result<Option<Job>> {
    let job_api = test.job_api();
    let jobs = job_api
        .list(&ListParams::default().fields(&format!("metadata.name={}", test.name())))
        .await
        .context(error::KubeClient {
            action: "list jobs",
            test_name: test.name(),
        })?;
    Ok(jobs.items.into_iter().next())
}

/// Checks whether the TestSys `Test`'s k8s `Job` exists. If not, the test's `Lifecycle` state is
/// set to 'deleted'.
///
/// # Preconditions
///
/// Assumes that deletion of the `Job` has been started but not finished. In other words, assumes
/// that the pod finalizer is present and that it is appropriate to update the `Lifecycle` to
/// `TestPodDeleted`.
///
pub(crate) async fn check_test_pod_deletion(test: &mut TestInterface) -> Result<ReconcilerAction> {
    if get_job(test).await?.is_some() {
        // The job is not deleted yet, no status change.
        trace!("Test pod is being deleted for test '{}'.", test.name());
    } else {
        // The job does not exist, it must be deleted.
        trace!("Test pod deleted for test '{}'.", test.name());
        let mut status = test.controller_status().into_owned();
        status.lifecycle = Lifecycle::TestPodDeleted;
        test.set_controller_status(status).await?;
    }
    Ok(REQUEUE)
}

// =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^= =^..^=
// private

/// We run the test pod using a k8s `Job`. Jobs can run many containers and provide counts of how
/// many containers are running or have completed (succeeded or failed). We are only running one
/// container, so it is helpful to transform those counts into a simple enumeration of our job's
/// state.
enum JobState {
    Unknown,
    Running,
    Failed,
    Succeeded,
}

impl JobState {
    /// Transform the container counts in `job.status` to a `JobState`
    fn from_job(job: &Job) -> Result<Self> {
        // Return early if `job.status` is somehow `None`.
        let status = match &job.status {
            None => {
                return Ok(Self::Unknown);
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
            return Ok(Self::Unknown);
        }

        // There should be exactly one container.
        ensure!(
            running + succeeded + failed == 1,
            error::TooManyJobContainers {
                test_name: job
                    .metadata
                    .name
                    .as_ref()
                    .map_or("name unknown", |name| name.as_str()),
                running,
                succeeded,
                failed
            }
        );

        if running == 1 {
            Ok(Self::Running)
        } else if succeeded == 1 {
            Ok(Self::Succeeded)
        } else {
            Ok(Self::Failed)
        }
    }
}

/// Creates a k8s job to run the test pod.
async fn create_job(test: &TestInterface) -> Result<Job> {
    let labels = create_labels(test);

    // Definition of the test pod's job.
    let job = Job {
        metadata: ObjectMeta {
            name: Some(test.name().to_owned()),
            namespace: Some(NAMESPACE.to_owned()),
            labels: Some(labels.clone()),
            ..ObjectMeta::default()
        },
        spec: Some(JobSpec {
            backoff_limit: Some(0),
            template: PodTemplateSpec {
                spec: Some(PodSpec {
                    containers: vec![Container {
                        name: test.name().to_owned(),
                        image: Some(test.agent().image.to_owned()),
                        env: Some(vec![EnvVar {
                            name: ENV_TEST_NAME.to_string(),
                            value: Some(test.name().to_owned()),
                            ..EnvVar::default()
                        }]),
                        ..Container::default()
                    }],
                    restart_policy: Some(String::from("Never")),
                    image_pull_secrets: Some(vec![LocalObjectReference {
                        name: test.agent().pull_secret.to_owned(),
                    }]),
                    service_account: Some(TEST_AGENT_SERVICE_ACCOUNT.to_owned()),
                    ..PodSpec::default()
                }),
                metadata: Some(ObjectMeta {
                    labels: Some(labels),
                    ..ObjectMeta::default()
                }),
            },
            ..JobSpec::default()
        }),
        ..Job::default()
    };

    // Deploy the test pod in a job.
    Ok(test
        .job_api()
        .create(&PostParams::default(), &job)
        .await
        .context(error::KubeClient {
            test_name: test.name(),
            action: "create job",
        })?)
}

/// Deletes a k8s `Job`. Updates the TestSys `Test`'s `Lifecycle` state to either 'deleting' or
/// 'deleted' depending on what k8s returns.
///
/// # Preconditions
///
/// The `Job` should exist otherwise k8s will return a 'not found' error.
///
async fn delete_job(test: &mut TestInterface) -> Result<()> {
    let api = test.job_api();
    let delete_return = api
        .delete(
            test.name(),
            &DeleteParams {
                propagation_policy: Some(PropagationPolicy::Foreground),
                ..DeleteParams::default()
            },
        )
        .await
        .context(error::KubeClient {
            test_name: test.name(),
            action: "delete job",
        })?;

    let mut status = test.controller_status().into_owned();

    // The delete function returns an `Either` enum where `Left` means deleting has started and
    // `Right` means the item has been fully deleted. Also, I can't seem to import the `Either` enum
    // getting "expected enum `either::Either`, found enum `Either`" which means I cannot write a
    // match statement.
    status.lifecycle = if delete_return.is_left() {
        Lifecycle::TestPodDeleting
    } else {
        trace!("Test pod deleted for test '{}'", test.name());
        Lifecycle::TestPodDeleted
    };
    test.set_controller_status(status).await
}

/// Creates the labels that we will add to the test pod deployment.
fn create_labels(test: &TestInterface) -> BTreeMap<String, String> {
    let mut labels = BTreeMap::new();
    insert(&mut labels, APP_NAME, test.agent().name.to_owned());
    insert(&mut labels, APP_INSTANCE, test.name());
    insert(&mut labels, APP_COMPONENT, TEST_AGENT);
    insert(&mut labels, APP_PART_OF, TESTSYS);
    insert(&mut labels, APP_MANAGED_BY, CONTROLLER);
    insert(&mut labels, APP_CREATED_BY, CONTROLLER);
    insert(&mut labels, LABEL_TEST_NAME, test.name());
    insert(&mut labels, LABEL_TEST_UID, test.id());
    labels
}

/// Convenience function so we don't have to call `to_owned` all over the place.
fn insert<S1, S2>(map: &mut BTreeMap<String, String>, k: S1, v: S2) -> Option<String>
where
    S1: Into<String>,
    S2: Into<String>,
{
    map.insert(k.into(), v.into())
}
