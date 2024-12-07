use crate::model::entities::{Interval, Partition};
use crate::model::{analyzer::analyze_files, journal::Closer};
use crate::report::multiperiod_balance::MultiperiodBalance;
use crate::syntax::parse_files;
use chrono::{NaiveDate, Utc};
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
        let partition = Partition::from_interval(journal.period().unwrap(), Interval::Monthly);

        let mut closer = Closer::new(
            partition.start_dates(),
            journal.registry.account_id("Equity:Equity").unwrap(),
        );
        let mut t =
            MultiperiodBalance::new(journal.registry.clone(), vec![Utc::now().date_naive()]);
        journal
            .query()
            .flat_map(|b| closer.process(b))
            .for_each(|b| {
                // println!("{}", b.account);
                t.register(b);
            });
        t.print();

        Ok(())
    }
}
