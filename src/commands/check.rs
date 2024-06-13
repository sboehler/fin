use crate::model::analyzer::analyze_files;
use crate::process::{check, compute_gains, compute_prices, valuate_transactions};
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
        let journal = analyze_files(&syntax_trees)?;
        check(&journal)?;
        if let Some(name) = &self.valuation {
            let commodity = journal.registry.borrow_mut().commodity(name)?;
            compute_prices(&journal, Some(commodity.clone()))?;
            valuate_transactions(&journal, Some(commodity.clone()))?;
            compute_gains(&journal, Some(commodity))?
        }
        Ok(())
    }
}
