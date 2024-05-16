use crate::model::analyzer::analyze_files;
use crate::process::check;
use crate::syntax::parse_files;
use clap::Args;
use std::{error::Error, path::PathBuf};

#[derive(Args)]
pub struct Command {
    journal: PathBuf,
}

impl Command {
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        let syntax_trees = parse_files(&self.journal)?;
        let mut journal = analyze_files(&syntax_trees)?;
        check(&mut journal)?;
        Ok(())
    }
}
