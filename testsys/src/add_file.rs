use crate::error::{self, Result};
use crate::k8s::create_or_update;
use kube::{Api, Client};
use model::{constants::NAMESPACE, Resource};
use serde::Deserialize;
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
        // Create the resource objects from its path.
        let resources_string =
            std::fs::read_to_string(&self.path).context(error::File { path: &self.path })?;

        let resources = Api::<Resource>::namespaced(k8s_client, NAMESPACE);
        for resource_doc in serde_yaml::Deserializer::from_str(&resources_string) {
            let value = serde_yaml::Value::deserialize(resource_doc)
                .context(error::ResourceProviderFileParse { path: &self.path })?;
            let resource: Resource = serde_yaml::from_value(value)
                .context(error::ResourceProviderFileParse { path: &self.path })?;
            let name = resource.metadata.name.clone();
            create_or_update(&resources, resource, "Resource Provider").await?;
            if let Some(name) = name {
                println!("Successfully added resource '{}'.", name);
            }
        }
        Ok(())
    }
}
