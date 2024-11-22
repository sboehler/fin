use crate::model::analyzer::analyze_files;
use crate::syntax::parse_files;
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
        let mut journal = analyze_files(&syntax_trees)?;
        journal.check()?;
        let val = self
            .valuation
            .as_ref()
            .map(|s| journal.registry.commodity(s))
            .transpose()?;
        journal.process(val.as_ref(), None)?;
        for b in journal.query() {
            println!("{}", b.description)
        }
        Ok(())
    }
}
