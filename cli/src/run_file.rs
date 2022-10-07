use anyhow::{Context, Result};
use clap::{value_parser, Parser};
use model::test_manager::{read_manifest, TestManager};
use std::path::PathBuf;

/// Run a test stored in a YAML file at `path`.
#[derive(Debug, Parser)]
pub(crate) struct RunFile {
    /// Path to test crd YAML file.
    #[clap(value_parser = value_parser!(PathBuf))]
    path: PathBuf,
}

impl RunFile {
    pub(crate) async fn run(&self, client: TestManager) -> Result<()> {
        // Create the resource objects from its path.
        let crds = read_manifest(&self.path).context("Unable to read manifest")?;
        for crd in crds {
            let name = crd.name();
            client
                .create_object(crd)
                .await
                .context("Unable to create object")?;
            if let Some(name) = name {
                println!("Successfully added '{}'.", name);
            }
        }
        Ok(())
    }
}
