use crate::error::Result;
use crate::job::JobState;
use crate::test_controller::context::TestInterface;
use anyhow::Context;
use kube::Api;
use log::trace;
use model::constants::{FINALIZER_MAIN, FINALIZER_TEST_JOB, NAMESPACE};
use model::{CrdExt, Resource, ResourceAction, TaskState};
use std::fmt::{Display, Formatter};

/// The action that the controller needs to take in order to reconcile the `Test`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) enum Action {
    Initialize,
    AddMainFinalizer,
    WaitForResources,
    RegisterResourceCreationError(String),
    AddJobFinalizer,
    StartTest,
    WaitForTest,
    DeleteJob,
    RemoveJobFinalizer,
    RemoveMainFinalizer,
    TestDone,
    Error(ErrorState),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) enum ErrorState {
    ResourceErrorExists(String),
    Zombie,
    TestError(String),
    JobFailure,
    JobExitBeforeDone,
    HandleJobRemovedBeforeDone,
}

impl Display for ErrorState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorState::ResourceErrorExists(e) => Display::fmt(e, f),
            ErrorState::Zombie => Display::fmt(
                "The main finalizer has been removed but the object still exists",
                f,
            ),
            ErrorState::TestError(e) => Display::fmt(e, f),
            ErrorState::JobFailure => Display::fmt("The job failed", f),
            ErrorState::JobExitBeforeDone => {
                Display::fmt("The test agent exited before marking the test complete", f)
            }
            ErrorState::HandleJobRemovedBeforeDone => {
                Display::fmt("The job was removed before the test completed", f)
            }
        }
    }
}

/// Inspect the `test` to determine which `Action` the controller should take.
pub(super) async fn determine_action(t: &TestInterface) -> Result<Action> {
    if t.test().is_delete_requested() {
        return determine_delete_action(t).await;
    }

    if t.test().status.is_none() {
        return Ok(Action::Initialize);
    }

    if !t.test().has_finalizer(FINALIZER_MAIN) {
        return Ok(Action::AddMainFinalizer);
    }

    let agent_status = t.test().agent_status();
    match agent_status.task_state {
        TaskState::Unknown => task_not_done_action(t, false).await,
        TaskState::Running => task_not_done_action(t, true).await,
        TaskState::Completed => Ok(Action::TestDone),
        TaskState::Error => Ok(Action::Error(ErrorState::TestError(
            t.test().agent_error().unwrap_or("Unknown error").to_owned(),
        ))),
    }
}

/// Determines what we should do next if the TestSys `Test` CRD has been marked for deletion.
///
/// # Preconditions
///
/// This function assumes that the test has been marked for deletion. This is checked in debug
/// builds but not in release builds.
///
pub(super) async fn determine_delete_action(t: &TestInterface) -> Result<Action> {
    debug_assert!(t.test().is_delete_requested());
    let job_state = t.get_job_state().await?;
    if !matches!(job_state, JobState::None) {
        Ok(Action::DeleteJob)
    } else if t.test().has_finalizer(FINALIZER_TEST_JOB) {
        Ok(Action::RemoveJobFinalizer)
    } else if t.test().has_finalizer(FINALIZER_MAIN) {
        Ok(Action::RemoveMainFinalizer)
    } else {
        Ok(Action::Error(ErrorState::Zombie))
    }
}

enum Resources {
    NotReady,
    Ready,
    Error(String),
}

async fn resource_readiness(t: &TestInterface) -> Result<Resources> {
    if t.test().spec.resources.is_empty() {
        return Ok(Resources::Ready);
    }
    let resource_client: Api<Resource> = Api::namespaced(t.k8s_client(), NAMESPACE);
    let resources_names = &t.test().spec.resources;
    for resource_name in resources_names {
        let result = resource_client.get(resource_name).await;
        if let Err(kube::Error::Api(response)) = &result {
            if response.code == 404 {
                return Ok(Resources::Error(format!(
                    "Resource '{}' not found",
                    resource_name
                )));
            }
        }
        let resource =
            result.with_context(|| format!("Unable to get resource '{}'", resource_name))?;
        if let Some(error) = resource.creation_error() {
            return Ok(Resources::Error(format!(
                "Error creating resource '{}': {}",
                resource_name,
                error.error.clone()
            )));
        }
        match resource.task_state(ResourceAction::Create) {
            TaskState::Unknown | TaskState::Running => return Ok(Resources::NotReady),
            TaskState::Completed => continue,
            TaskState::Error => {
                return Ok(Resources::Error(format!(
                    "Creation of resource '{}' failed",
                    resource_name
                )))
            }
        }
    }
    Ok(Resources::Ready)
}

async fn task_not_done_action(t: &TestInterface, is_task_state_running: bool) -> Result<Action> {
    if !is_task_state_running && !t.test().has_finalizer(FINALIZER_TEST_JOB) {
        return Ok(Action::AddJobFinalizer);
    }
    let job_state = t.get_job_state().await?;
    match job_state {
        JobState::None if !is_task_state_running => match resource_readiness(t).await? {
            Resources::NotReady => Ok(Action::WaitForResources),
            Resources::Error(s) => {
                if t.test().resource_error().is_some() {
                    Ok(Action::RegisterResourceCreationError(s))
                } else {
                    Ok(Action::Error(ErrorState::ResourceErrorExists(s)))
                }
            }
            Resources::Ready => Ok(Action::StartTest),
        },
        JobState::None => Ok(Action::Error(ErrorState::HandleJobRemovedBeforeDone)),
        JobState::Unknown => {
            trace!("Waiting for test agent '{}' container to start", t.name());
            Ok(Action::WaitForTest)
        }
        JobState::Running => {
            trace!("Test '{}' is running", t.name());
            Ok(Action::WaitForTest)
        }
        JobState::Failed => Ok(Action::Error(ErrorState::JobFailure)),
        JobState::Exited => Ok(Action::Error(ErrorState::JobExitBeforeDone)),
    }
}
