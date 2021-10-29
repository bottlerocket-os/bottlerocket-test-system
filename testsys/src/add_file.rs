use crate::error::{self, Result};
use crate::k8s::create_or_update;
use kube::{Api, Client};
use model::{constants::NAMESPACE, Resource};
use snafu::ResultExt;
use std::path::PathBuf;
use structopt::StructOpt;

/// Add a `Resource` stored in a YAML file at `path`.
#[derive(Debug, StructOpt)]
pub(crate) struct AddFile {
    /// Path to the resource provider YAML file.
    #[structopt(parse(from_os_str))]
    path: PathBuf,
}

impl AddFile {
    pub(crate) async fn run(&self, k8s_client: Client) -> Result<()> {
        // Create the test object from its path.
        let resource_file =
            std::fs::File::open(&self.path).context(error::File { path: &self.path })?;
        let resource = serde_yaml::from_reader(resource_file)
            .context(error::ResourceProviderFileParse { path: &self.path })?;

        let resources = Api::<Resource>::namespaced(k8s_client, NAMESPACE);

        create_or_update(&resources, resource, "Resource Provider").await?;

        Ok(())
    }
}
