use crate::error::Result;
use crate::job::JobState;
use crate::resource_controller::context::ResourceInterface;
use log::trace;
use model::constants::{FINALIZER_CREATION_JOB, FINALIZER_MAIN, FINALIZER_RESOURCE};
use model::{CrdExt, ResourceAction, TaskState};

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
    Wait,
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
    JobExited,
    JobFailed,
    JobRemoved,
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

    let task_state = r.resource().creation_task_state();
    match task_state {
        TaskState::Unknown => creation_not_done_action(r, false).await,
        TaskState::Running => creation_not_done_action(r, true).await,
        TaskState::Completed => creation_completed_action(r).await,
        TaskState::Error => Ok(CreationAction::Error(ErrorState::TaskFailed)),
    }
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
        JobState::Unknown | JobState::Running => Ok(CreationAction::Wait),
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
        JobState::Unknown | JobState::Running => Ok(DestructionAction::Wait),
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
