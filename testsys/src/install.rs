use crate::error::{self, Result};
use structopt::StructOpt;

/// TODO - This is a placeholder.
#[derive(Debug, StructOpt)]
pub(crate) struct Install {}

impl Install {
    pub(crate) async fn run(&self) -> Result<()> {
        error::Placeholder {}.fail()
    }
}
