use crate::process::check;
use crate::syntax::parse_files;
use crate::{model::analyzer::analyze_files, process::compute_valuation};
use clap::Args;
use std::{error::Error, path::PathBuf};

#[derive(Args)]
pub struct Command {
    journal: PathBuf,

    #[arg(short, long, value_name = "COMMODITY")]
    valuation: Option<String>,
}

impl Command {
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        let syntax_trees = parse_files(&self.journal)?;
        let journal = analyze_files(&syntax_trees)?;
        check(&journal)?;
        if let Some(name) = &self.valuation {
            let commodity = journal.registry.borrow_mut().commodity(name)?;
            compute_valuation(&journal, Some(commodity))?
        }
        Ok(())
    }
}
