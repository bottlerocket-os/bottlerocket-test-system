use crate::constants::{NO_REQUEUE, REQUEUE, REQUEUE_SLOW};
use crate::error::{ReconciliationResult, Result};
use crate::job::{JobBuilder, JobType};
use crate::test_controller::action::{determine_action, Action};
use crate::test_controller::context::{Context, TestInterface};
use anyhow::Context as AnyhowContext;
use kube_runtime::controller::ReconcilerAction;
use log::{debug, error, trace};
use model::clients::CrdClient;
use model::constants::{ENV_TEST_NAME, FINALIZER_MAIN, FINALIZER_TEST_JOB, TEST_AGENT};
use model::Test;

/// `reconcile` is called when a new `Test` object arrives, or when a `Test` object has been
/// re-queued. This is the entrypoint to the controller logic.
pub(crate) async fn reconcile(t: Test, context: Context) -> ReconciliationResult<ReconcilerAction> {
    let mut t = TestInterface::new(t, context)?;
    let action = determine_action(&t).await?;
    trace!("action {:?}", action);
    match action {
        Action::Initialize => {
            t.test_client()
                .initialize_status(t.name())
                .await
                .context(format!("Unable to initialize status for '{}'", t.name()))?;
            Ok(REQUEUE)
        }
        // Action::Acknowledge => acknowledge_new_test(&mut test).await,
        Action::AddMainFinalizer => {
            t.test_client()
                .add_finalizer(FINALIZER_MAIN, t.test())
                .await
                .context(format!("Unable to add main finalizer for '{}'", t.name()))?;
            Ok(REQUEUE)
        }
        Action::WaitForResources => Ok(REQUEUE),
        Action::RegisterResourceCreationError(msg) => {
            t.test_client()
                .send_resource_error(t.name(), &msg)
                .await
                .context(format!(
                    "Unable to register creation error '{}' for '{}'",
                    msg,
                    t.name()
                ))?;
            Ok(REQUEUE_SLOW)
        }
        Action::WaitForDependency(_) => Ok(REQUEUE),
        Action::AddJobFinalizer => {
            t.test_client()
                .add_finalizer(FINALIZER_TEST_JOB, t.test())
                .await
                .context(format!("Unable to add job finalizer for '{}'", t.name()))?;
            Ok(REQUEUE)
        }
        Action::StartTest => {
            create_job(&mut t).await?;
            Ok(REQUEUE)
        }
        Action::WaitForTest => Ok(REQUEUE),
        Action::DeleteJob => {
            t.delete_job().await?;
            Ok(REQUEUE)
        }
        Action::RemoveJobFinalizer => {
            t.test_client()
                .remove_finalizer(FINALIZER_TEST_JOB, t.test())
                .await
                .context(format!("Unable to remove job finalizer for '{}'", t.name()))?;
            Ok(REQUEUE)
        }
        Action::RemoveMainFinalizer => {
            t.test_client()
                .remove_finalizer(FINALIZER_MAIN, t.test())
                .await
                .context(format!(
                    "Unable to remove main finalizer for '{}'",
                    t.name()
                ))?;
            Ok(NO_REQUEUE)
        }
        Action::TestDone => {
            debug!("Test '{}' is done", t.name());
            Ok(REQUEUE_SLOW)
        }
        Action::Error(state) => {
            error!("Error state for test '{}': {}", t.name(), state);
            Ok(REQUEUE_SLOW)
        }
    }
}

/// Runs a k8s `Job` to run our test pod. Adds the pod finalizer to ensure we don't forget to clean
/// up the `Job` later.
///
/// # Preconditions
///
/// Assumes that the pod finalizer is not present. If it is, A duplicate finalizer error will occur.
///
pub(crate) async fn create_job(t: &mut TestInterface) -> Result<()> {
    debug!("Creating test job '{}'", t.name());
    JobBuilder {
        agent: &t.test().spec.agent,
        job_name: t.name(),
        job_type: JobType::TestAgent,
        component: TEST_AGENT,
        environment_variables: vec![(ENV_TEST_NAME, t.name().to_owned())],
    }
    .deploy(t.k8s_client())
    .await
    .context(format!("Unable to create job '{}'", t.name()))?;
    Ok(())
}
