use crate::{model::build_journal, syntax::parse_files};
use clap::Args;
use std::{error::Error, path::PathBuf};

#[derive(Args)]
pub struct Command {
    journal: PathBuf,
}

impl Command {
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        let files = parse_files(&self.journal)?;
        build_journal(&files)?;
        Ok(())
    }
}
