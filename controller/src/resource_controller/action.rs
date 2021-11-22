use crate::error::Result;
use crate::job::{JobState, TEST_START_TIME_LIMIT};
use crate::resource_controller::context::ResourceInterface;
use kube::ResourceExt;
use log::trace;
use model::clients::CrdClient;
use model::constants::{FINALIZER_CREATION_JOB, FINALIZER_MAIN, FINALIZER_RESOURCE};
use model::{CrdExt, ResourceAction, TaskState};
use parse_duration::parse;

/// The action that the controller needs to take in order to reconcile the [`Resource`].
#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) enum Action {
    Creation(CreationAction),
    Destruction(DestructionAction),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) enum CreationAction {
    Initialize,
    AddMainFinalizer,
    AddJobFinalizer,
    StartJob,
    WaitForDependency(String),
    WaitForCreation,
    AddResourceFinalizer,
    Done,
    Error(ErrorState),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) enum DestructionAction {
    RemoveCreationJob,
    RemoveCreationJobFinalizer,
    StartDestructionJob,
    Wait,
    RemoveDestructionJob,
    RemoveResourceFinalizer,
    RemoveMainFinalizer,
    Error(ErrorState),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) enum ErrorState {
    JobStart,
    JobExited,
    JobFailed,
    JobRemoved,
    JobTimeout,
    TaskFailed,
    Zombie,
}

pub(super) async fn action(r: &ResourceInterface) -> Result<Action> {
    if r.resource().is_delete_requested() {
        Ok(Action::Destruction(destruction_action(r).await?))
    } else {
        Ok(Action::Creation(creation_action(r).await?))
    }
}

async fn creation_action(r: &ResourceInterface) -> Result<CreationAction> {
    if r.resource().status.is_none() {
        return Ok(CreationAction::Initialize);
    }

    if !r.resource().has_finalizers() {
        return Ok(CreationAction::AddMainFinalizer);
    }

    if let Some(wait_action) = dependency_wait_action(r).await? {
        return Ok(wait_action);
    }

    let task_state = r.resource().creation_task_state();
    match task_state {
        TaskState::Unknown => creation_not_done_action(r, false).await,
        TaskState::Running => creation_not_done_action(r, true).await,
        TaskState::Completed => creation_completed_action(r).await,
        TaskState::Error => Ok(CreationAction::Error(ErrorState::TaskFailed)),
    }
}

async fn dependency_wait_action(r: &ResourceInterface) -> Result<Option<CreationAction>> {
    let depends_on = if let Some(depends_on) = &r.resource().spec.depends_on {
        if depends_on.is_empty() {
            return Ok(None);
        }
        depends_on
    } else {
        return Ok(None);
    };

    // Make sure each resource in depends_on is ready.
    // TODO - error if cyclical dependencies https://github.com/bottlerocket-os/bottlerocket-test-system/issues/156
    for needed in depends_on {
        // TODO - error if 404/not-found https://github.com/bottlerocket-os/bottlerocket-test-system/issues/157
        let needed_resource = r.resource_client().get(needed).await?;
        if needed_resource.created_resource().is_none() {
            return Ok(Some(CreationAction::WaitForDependency(
                needed_resource.name().to_owned(),
            )));
        }
    }
    Ok(None)
}

async fn creation_not_done_action(
    r: &ResourceInterface,
    is_task_state_running: bool,
) -> Result<CreationAction> {
    if !is_task_state_running && !r.resource().has_finalizer(FINALIZER_CREATION_JOB) {
        return Ok(CreationAction::AddJobFinalizer);
    }
    let job_state = r.get_job_state(ResourceAction::Create).await?;
    match job_state {
        JobState::None if !is_task_state_running => Ok(CreationAction::StartJob),
        JobState::None => Ok(CreationAction::Error(ErrorState::JobRemoved)),
        JobState::Unknown => Ok(CreationAction::WaitForCreation),
        JobState::Running(None) => Ok(CreationAction::WaitForCreation),
        JobState::Running(Some(duration)) => {
            if let Ok(std_duration) = duration.to_std() {
                if r.resource()
                    .spec
                    .agent
                    .timeout
                    .as_ref()
                    .map(|timeout| parse(timeout).map(|timeout| std_duration > timeout))
                    .unwrap_or(Ok(false))
                    .unwrap_or(false)
                {
                    return Ok(CreationAction::Error(ErrorState::JobTimeout));
                }
            }
            if r.resource().creation_task_state() == TaskState::Unknown
                && duration >= *TEST_START_TIME_LIMIT
            {
                return Ok(CreationAction::Error(ErrorState::JobStart));
            }
            Ok(CreationAction::WaitForCreation)
        }
        JobState::Failed => Ok(CreationAction::Error(ErrorState::JobFailed)),
        JobState::Exited => Ok(CreationAction::Error(ErrorState::JobExited)),
    }
}

async fn creation_completed_action(r: &ResourceInterface) -> Result<CreationAction> {
    if !r.resource().has_finalizer(FINALIZER_RESOURCE) {
        Ok(CreationAction::AddResourceFinalizer)
    } else {
        Ok(CreationAction::Done)
    }
}

async fn destruction_action(r: &ResourceInterface) -> Result<DestructionAction> {
    if let Some(creation_cleanup_action) = creation_cleanup_action(r).await? {
        Ok(creation_cleanup_action)
    } else if r.resource().has_finalizer(FINALIZER_RESOURCE) {
        destruction_action_with_resources(r).await
    } else {
        destruction_action_without_resources(r).await
    }
}

async fn creation_cleanup_action(r: &ResourceInterface) -> Result<Option<DestructionAction>> {
    if !r.resource().has_finalizer(FINALIZER_CREATION_JOB) {
        return Ok(None);
    }
    let job_state = r.get_job_state(ResourceAction::Create).await?;
    if matches!(job_state, JobState::None) {
        Ok(Some(DestructionAction::RemoveCreationJobFinalizer))
    } else {
        Ok(Some(DestructionAction::RemoveCreationJob))
    }
}

async fn destruction_action_with_resources(r: &ResourceInterface) -> Result<DestructionAction> {
    match r.resource().destruction_task_state() {
        TaskState::Unknown => destruction_not_done_action(r, false).await,
        TaskState::Running => destruction_not_done_action(r, true).await,
        TaskState::Completed => {
            let job_state = r.get_job_state(ResourceAction::Destroy).await?;
            trace!("deciding what to do with job_state: {:?}", job_state);
            let job_exists = !matches!(job_state, JobState::None);
            trace!("job exists: {:?}", job_exists);
            if job_exists {
                Ok(DestructionAction::RemoveDestructionJob)
            } else {
                Ok(DestructionAction::RemoveResourceFinalizer)
            }
        }
        TaskState::Error => Ok(DestructionAction::Error(ErrorState::TaskFailed)),
    }
}

async fn destruction_not_done_action(
    r: &ResourceInterface,
    is_task_state_running: bool,
) -> Result<DestructionAction> {
    let job_state = r.get_job_state(ResourceAction::Destroy).await?;
    match job_state {
        JobState::None if !is_task_state_running => Ok(DestructionAction::StartDestructionJob),
        JobState::None => Ok(DestructionAction::Error(ErrorState::JobRemoved)),
        JobState::Unknown => Ok(DestructionAction::Wait),
        JobState::Running(None) => Ok(DestructionAction::Wait),
        JobState::Running(Some(duration)) => {
            if let Ok(std_duration) = duration.to_std() {
                if r.resource()
                    .spec
                    .agent
                    .timeout
                    .as_ref()
                    .map(|timeout| parse(timeout).map(|timeout| std_duration > timeout))
                    .unwrap_or(Ok(false))
                    .unwrap_or(false)
                {
                    return Ok(DestructionAction::Error(ErrorState::JobTimeout));
                }
            }
            if r.resource().destruction_task_state() == TaskState::Unknown
                && duration >= *TEST_START_TIME_LIMIT
            {
                return Ok(DestructionAction::Error(ErrorState::JobStart));
            }
            Ok(DestructionAction::Wait)
        }
        JobState::Failed => Ok(DestructionAction::Error(ErrorState::JobFailed)),
        JobState::Exited => Ok(DestructionAction::Error(ErrorState::JobExited)),
    }
}

async fn destruction_action_without_resources(r: &ResourceInterface) -> Result<DestructionAction> {
    let job_state = r.get_job_state(ResourceAction::Destroy).await?;
    let job_exists = !matches!(job_state, JobState::None);
    if job_exists {
        Ok(DestructionAction::RemoveDestructionJob)
    } else if r.resource().has_finalizer(FINALIZER_MAIN) {
        Ok(DestructionAction::RemoveMainFinalizer)
    } else {
        Ok(DestructionAction::Error(ErrorState::Zombie))
    }
}
