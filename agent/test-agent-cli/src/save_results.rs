use crate::error::{ArchiveSnafu, Result};
use argh::FromArgs;
use copy_dir::copy_dir;
use log::error;
use snafu::ResultExt;
use std::path::Path;
use test_agent::DefaultClient;
use testsys_model::constants::TESTSYS_RESULTS_DIRECTORY;

#[derive(Debug, FromArgs, PartialEq)]
#[argh(
    subcommand,
    name = "save-results",
    description = "save the results file or directory in tar archive"
)]
pub(crate) struct SaveResults {
    #[argh(short = 'd', option, description = "directory to add in tar archive")]
    directory: Vec<String>,
    #[argh(short = 'f', option, description = "file to add in tar archive")]
    file: Vec<String>,
}

impl SaveResults {
    pub(crate) async fn run(&self, _k8s_client: DefaultClient) -> Result<()> {
        for dir in &self.directory {
            copy_dir(dir, TESTSYS_RESULTS_DIRECTORY.to_owned() + "/" + dir)
                .context(ArchiveSnafu)?;
        }

        for file in &self.file {
            move_file(file, TESTSYS_RESULTS_DIRECTORY.to_owned() + "/" + file).await?;
        }

        if self.directory.is_empty() && self.file.is_empty() {
            error!("Invalid arguments were provided. One of `--file`, `--directory` must be used.");
        }

        Ok(())
    }
}

async fn move_file<P, Q>(from: P, to: Q) -> Result<()>
where
    P: AsRef<Path>,
    Q: AsRef<Path>,
{
    let from = from.as_ref();
    if !from.exists() {
        error!("Results file does not exist");
    }

    if !from.is_file() {
        println!("Results file path is not a file");
    }

    std::fs::copy(from, to).context(ArchiveSnafu)?;
    Ok(())
}
