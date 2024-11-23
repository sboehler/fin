use crate::model::analyzer::analyze_files;
use crate::syntax::parse_files;
use chrono::NaiveDate;
use clap::Args;
use std::{error::Error, path::PathBuf};

#[derive(Args)]
pub struct Command {
    journal: PathBuf,

    #[arg(short, long, value_name = "COMMODITY")]
    valuation: Option<String>,

    #[arg(short, long, value_name = "FROM_DATE")]
    from_date: Option<NaiveDate>,
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
        journal
            .query()
            .filter_map(|mut r| {
                if !self.from_date.map(|d| r.date >= d).unwrap_or(true) {
                    return None;
                }
                r.account = journal.registry.shorten(r.account, 0)?;
                Some(r)
            })
            .for_each(|b| println!("{}", b.account));
        Ok(())
    }
}
