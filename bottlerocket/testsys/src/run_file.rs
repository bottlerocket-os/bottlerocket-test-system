use crate::error::{self, Result};
use kube::Client;
use serde::{Deserialize, Serialize};
use snafu::ResultExt;
use std::path::PathBuf;
use structopt::StructOpt;
use testsys_model::{
    clients::{CrdClient, ResourceClient, TestClient},
    Resource, Test,
};

/// Run a test stored in a YAML file at `path`.
#[derive(Debug, StructOpt)]
pub(crate) struct RunFile {
    /// Path to test crd YAML file.
    #[structopt(parse(from_os_str))]
    path: PathBuf,
}

impl RunFile {
    pub(crate) async fn run(&self, k8s_client: Client) -> Result<()> {
        // Create the resource objects from its path.
        let manifest_string =
            std::fs::read_to_string(&self.path).context(error::FileSnafu { path: &self.path })?;
        let tests = TestClient::new_from_k8s_client(k8s_client.clone());
        let resources = ResourceClient::new_from_k8s_client(k8s_client);
        for crd_doc in serde_yaml::Deserializer::from_str(&manifest_string) {
            let value = serde_yaml::Value::deserialize(crd_doc)
                .context(error::ResourceProviderFileParseSnafu { path: &self.path })?;
            let crd: Crd = serde_yaml::from_value(value)
                .context(error::ResourceProviderFileParseSnafu { path: &self.path })?;
            let name = crd.name();
            match crd {
                Crd::Test(test) => {
                    Crd::Test(tests.create(test).await.context(error::CreateTestSnafu)?)
                }
                Crd::Resource(resource) => Crd::Resource(
                    resources
                        .create(resource)
                        .await
                        .context(error::CreateResourceSnafu)?,
                ),
            };
            if let Some(name) = name {
                println!("Successfully added '{}'.", name);
            }
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
enum Crd {
    Test(Test),
    Resource(Resource),
}

impl Crd {
    fn name(&self) -> Option<String> {
        match self {
            Self::Test(test) => test.metadata.name.to_owned(),
            Self::Resource(resource) => resource.metadata.name.to_owned(),
        }
    }
}
