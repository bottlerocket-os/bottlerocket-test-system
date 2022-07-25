use crate::error::{self, ArchiveSnafu, Result};
use crate::Client;
use argh::FromArgs;
use snafu::ResultExt;
use std::fs::File;
use std::time::Duration;
use tar::Builder;
use tokio::time::sleep;

#[derive(Debug, FromArgs, PartialEq)]
#[argh(subcommand, name = "terminate", description = "complete the test")]
pub(crate) struct Terminate {}

impl Terminate {
    pub(crate) async fn run(&self, k8s_client: Client) -> Result<()> {
        self.tar_results(&k8s_client).await?;
        k8s_client.send_test_completed().await?;

        self.loop_while_keep_running_is_true(&k8s_client).await;

        Ok(())
    }

    async fn loop_while_keep_running_is_true(&self, k8s_client: &Client) {
        loop {
            sleep(Duration::from_secs(10)).await;
            if !self.keep_running(k8s_client).await {
                println!("'keep_running' has been set to false, exiting.");
                return;
            }
        }
    }

    async fn keep_running(&self, k8s_client: &Client) -> bool {
        match k8s_client.keep_running().await {
            Err(e) => {
                println!("Unable to communicate with Kuberenetes: '{}'", e);
                // If we can't communicate Kubernetes, its safest to
                // stay running in case some debugging is needed.
                true
            }
            Ok(value) => value,
        }
    }

    async fn tar_results(&self, k8s_client: &Client) -> Result<()> {
        let results_dir = k8s_client.results_directory().await?;

        let tar = File::create(k8s_client.results_file().await?).context(ArchiveSnafu)?;

        let mut archive = Builder::new(tar);
        archive
            .append_dir_all("test-results", results_dir)
            .context(error::ArchiveSnafu)?;
        archive.into_inner().context(error::ArchiveSnafu)?;
        Ok(())
    }
}
