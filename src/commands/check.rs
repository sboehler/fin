use crate::model::entities::{Interval, Partition};
use crate::model::{analyzer::analyze_files, journal::Closer};
use crate::report::balance::{Aligner, DatedPositions, MultiperiodTree, Shortener};
use crate::report::table::TextRenderer;
use crate::syntax::parse_files;
use chrono::NaiveDate;
use clap::Args;
use regex::Regex;
use std::borrow::BorrowMut;
use std::io::{stdout, Write};
use std::num::ParseIntError;
use std::str::FromStr;
use std::usize;
use std::{error::Error, path::PathBuf};

#[derive(Args)]
pub struct Command {
    journal: PathBuf,

    #[arg(short, long)]
    valuation: Option<String>,

    #[arg(short, long)]
    mapping: Vec<Mapping>,

    #[arg(long)]
    last: Option<usize>,

    #[arg(short, long)]
    from_date: Option<NaiveDate>,

    #[command(flatten)]
    period: Period,
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
        let partition =
            Partition::from_interval(journal.period().unwrap(), self.period.to_interval());

        let mut dates = partition
            .end_dates()
            .iter()
            .rev()
            .take(self.last.map(|v| v + 1).unwrap_or(usize::MAX))
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
        let shortener = Shortener::new(
            journal.registry.clone(),
            self.mapping
                .iter()
                .map(|m| (m.regex.clone(), m.level))
                .collect(),
        );
        let mut multiperiod_tree = MultiperiodTree::new(dates.clone(), journal.registry.clone());
        let test = dated_positions
            .iter()
            .flat_map(|(k, v)| shortener.shorten(*k).map(|k| (k, v)));
        multiperiod_tree.extend(test);
        let table = multiperiod_tree.render();
        let renderer = TextRenderer { table, round: 0 };
        let mut lock = stdout().lock();
        renderer.render(lock.borrow_mut()).unwrap();
        lock.flush()?;
        Ok(())
    }
}

#[derive(Args)]
#[group(multiple = false)]
struct Period {
    #[arg(long)]
    days: bool,
    #[arg(long)]
    weeks: bool,
    #[arg(long)]
    months: bool,
    #[arg(long)]
    quarters: bool,
    #[arg(long)]
    years: bool,
}

impl Period {
    fn to_interval(&self) -> Interval {
        if self.days {
            Interval::Daily
        } else if self.weeks {
            Interval::Weekly
        } else if self.months {
            Interval::Monthly
        } else if self.quarters {
            Interval::Quarterly
        } else if self.years {
            Interval::Yearly
        } else {
            Interval::Single
        }
    }
}

#[derive(Clone)]
struct Mapping {
    regex: Regex,
    level: usize,
}

impl FromStr for Mapping {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let mut parts = s.split(',');
        let levels = parts
            .next()
            .ok_or(format!("invalid mapping: {}", s))?
            .parse()
            .map_err(|e: ParseIntError| e.to_string())?;
        let regex = Regex::new(parts.next().unwrap_or(".*")).map_err(|e| e.to_string())?;
        Ok(Mapping {
            regex,
            level: levels,
        })
    }
}
