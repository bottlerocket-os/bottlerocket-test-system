use anyhow::{Context, Result};
use clap::Parser;
use model::test_manager::TestManager;

/// The uninstall subcommand is responsible for removing all testsys components from a k8s cluster.
#[derive(Debug, Parser)]
pub(crate) struct Uninstall {}

impl Uninstall {
    pub(crate) async fn run(self, client: TestManager) -> Result<()> {
        client.uninstall().await.context(
            "Unable to uninstall testsys from the cluster. (Some artifacts may be left behind)",
        )?;

        println!("testsys components were successfully uninstalled.");

        Ok(())
    }
}
