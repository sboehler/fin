use crate::model::build_journal;
use crate::model::entities::Interval;
use crate::report::balance::{Mapping, ReportAmount, ReportBuilder};
use crate::report::table::TextRenderer;
use crate::syntax::parse_files;
use chrono::{Local, NaiveDate};
use clap::Args;
use regex::Regex;
use std::borrow::BorrowMut;
use std::io::{Write, stdout};
use std::{error::Error, path::PathBuf};

#[derive(Args)]
pub struct Command {
    path: PathBuf,

    #[arg(short, long)]
    valuation: Option<String>,

    #[arg(short, long)]
    mapping: Vec<Mapping>,

    #[arg(short, long)]
    show_commodities: Vec<Regex>,

    #[arg(long)]
    last: Option<usize>,

    #[arg(long)]
    diff: bool,

    #[arg(short, long)]
    from: Option<NaiveDate>,

    #[arg(short, long)]
    to: Option<NaiveDate>,

    #[command(flatten)]
    period: PeriodArgs,

    #[arg(short, long)]
    quantity: bool,

    #[arg(long)]
    round: Option<usize>,
}

impl Command {
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        let syntax_trees = parse_files(&self.path)?;
        let mut journal = build_journal(&syntax_trees)?;
        journal.check()?;
        let valuation = self
            .valuation
            .as_ref()
            .map(|s| journal.registry().commodity_id(s))
            .transpose()?;
        journal.process(valuation)?;

        let builder = ReportBuilder {
            from: self.from,
            to: self.to.unwrap_or_else(|| Local::now().date_naive()),
            num_periods: self.last,
            period: self.period.to_interval(),
            mapping: self.mapping.clone(),
            cumulative: !self.diff,
            show_commodities: self.show_commodities.clone(),
            report_amount: match self.quantity {
                true => ReportAmount::Quantity,
                false => ReportAmount::Value,
            },
        };
        let report = builder.build(&journal);
        let renderer = TextRenderer::new(report.to_table(), self.round.unwrap_or_default());
        let mut lock = stdout().lock();
        renderer.render(lock.borrow_mut()).unwrap();
        lock.flush()?;
        Ok(())
    }
}

#[derive(Args)]
#[group(multiple = false)]
struct PeriodArgs {
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

impl PeriodArgs {
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
