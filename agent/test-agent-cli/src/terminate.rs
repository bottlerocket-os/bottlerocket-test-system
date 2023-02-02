use crate::error::{self, ArchiveSnafu, ClientSnafu, Result};
use argh::FromArgs;
use log::{error, info};
use snafu::ResultExt;
use std::fs::File;
use std::path::PathBuf;
use std::time::Duration;
use tar::Builder;
use test_agent::{Client, DefaultClient};
use testsys_model::constants::TESTSYS_RESULTS_DIRECTORY;
use tokio::time::sleep;

#[derive(Debug, FromArgs, PartialEq)]
#[argh(subcommand, name = "terminate", description = "complete the test")]
pub(crate) struct Terminate {}

impl Terminate {
    pub(crate) async fn run(&self, k8s_client: DefaultClient) -> Result<()> {
        // Wrap all the test results in a tar archive
        self.tar_results(&k8s_client).await?;

        // Mark task_status as complete, in either condition: Test results sent or not.
        // Resource status will be marked as "noTests" in case of no test results
        k8s_client
            .send_test_completed()
            .await
            .context(ClientSnafu)?;

        self.loop_while_keep_running_is_true(&k8s_client).await;

        Ok(())
    }

    async fn loop_while_keep_running_is_true(&self, k8s_client: &DefaultClient) {
        info!("Waiting for keep running flag");
        loop {
            sleep(Duration::from_secs(10)).await;
            if !self.keep_running(k8s_client).await {
                info!("'keep_running' has been set to false, exiting.");
                return;
            }
        }
    }

    async fn keep_running(&self, k8s_client: &DefaultClient) -> bool {
        match k8s_client.keep_running().await {
            Err(e) => {
                error!("Unable to communicate with Kuberenetes: '{}'", e);
                // If we can't communicate Kubernetes, its safest to
                // stay running in case some debugging is needed.
                true
            }
            Ok(value) => value,
        }
    }

    async fn tar_results(&self, k8s_client: &DefaultClient) -> Result<()> {
        let results_dir = PathBuf::from(TESTSYS_RESULTS_DIRECTORY);
        let tar = File::create(k8s_client.results_file().await.context(ClientSnafu)?)
            .context(ArchiveSnafu)?;

        let mut archive = Builder::new(tar);
        archive
            .append_dir_all("test-results", results_dir)
            .context(error::ArchiveSnafu)?;
        archive.into_inner().context(error::ArchiveSnafu)?;
        Ok(())
    }
}
