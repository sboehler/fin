use crate::syntax::file::parse_files;
use clap::Args;
use std::{error::Error, path::PathBuf};

#[derive(Args)]
pub struct Command {
    journal: PathBuf,
}

impl Command {
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        parse_files(&self.journal)?;
        Ok(())
    }
}
