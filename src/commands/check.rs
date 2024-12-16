use crate::model::entities::{Interval, Partition};
use crate::model::{analyzer::analyze_files, journal::Closer};
use crate::report::report::{Aligner, DatedPositions, MultiperiodTree, Shortener};
use crate::report::table::TextRenderer;
use crate::syntax::parse_files;
use chrono::NaiveDate;
use clap::Args;
use std::borrow::BorrowMut;
use std::io::{stdout, Write};
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
            .map(|s| journal.registry.commodity_id(s))
            .transpose()?;
        journal.process(val.as_ref(), None)?;
        let partition = Partition::from_interval(journal.period().unwrap(), Interval::Monthly);

        let mut dates = partition
            .end_dates()
            .iter()
            .rev()
            .take(12)
            .cloned()
            .collect::<Vec<_>>();
        dates.reverse();

        let mut closer = Closer::new(
            partition.start_dates(),
            journal.registry.account_id("Equity:Equity").unwrap(),
        );
        let aligner = Aligner::new(dates.clone());
        let dated_positions = journal
            .query()
            .flat_map(|row| closer.process(row))
            .flat_map(|row| aligner.align(row))
            .sum::<DatedPositions>();
        let shortener = Shortener::new(journal.registry.clone(), Vec::new());
        let mut multiperiod_tree = MultiperiodTree::new(dates.clone(), journal.registry.clone());
        let test = dated_positions
            .iter()
            .flat_map(|(k, v)| shortener.shorten(*k).map(|k| (k, v)));
        multiperiod_tree.extend(test);
        let table = multiperiod_tree.render();
        let renderer = TextRenderer { table, round: 2 };
        let mut lock = stdout().lock();
        renderer.render(lock.borrow_mut()).unwrap();
        lock.flush()?;
        Ok(())
    }
}
