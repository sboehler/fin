//! Lists the bookings of a group of accounts in chronological order.

use std::borrow::BorrowMut;
use std::io::{Write, stdout};
use std::{error::Error, path::PathBuf};

use chrono::{Local, NaiveDate};
use clap::Args;

use crate::model::build_journal;
use crate::report::mapping::{AccountMapper, Mapping};
use crate::report::register::RegisterBuilder;
use crate::report::table::TextRenderer;
use crate::syntax::parse_files;

/// Lists the bookings of a group of accounts by date.
#[derive(Args)]
pub struct Command {
    /// The journal to read. Its includes are followed.
    path: PathBuf,

    /// List the bookings of this account and of the accounts below it.
    /// Movements inside that group are left out.
    account: String,

    /// Show the value of every booking in this commodity instead of the
    /// quantity booked.
    #[arg(short, long)]
    valuation: Option<String>,

    /// Shorten the accounts shown to LEVEL segments. Repeatable.
    #[arg(short, long, value_name = "LEVEL,REGEX")]
    mapping: Vec<Mapping>,

    /// Substitute a virtual account for the accounts it matches. Repeatable.
    #[arg(short = 'r', long)]
    vaccounts: Vec<String>,

    /// Start of the period. Defaults to the first transaction.
    #[arg(short, long)]
    from: Option<NaiveDate>,

    /// End of the period. Defaults to today.
    #[arg(short, long)]
    to: Option<NaiveDate>,

    /// Wrap the description at this many characters.
    #[arg(long, default_value_t = 80)]
    width: usize,

    /// Decimal places. Defaults to the two of a currency amount with
    /// `--valuation`, and to what the quantities need to be exact without
    /// it.
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
        let mapper = AccountMapper::new(journal.registry(), self.mapping.clone(), &self.vaccounts)?;
        let builder = RegisterBuilder {
            from: self.from,
            to: self.to.unwrap_or_else(|| Local::now().date_naive()),
            account: self.account.clone(),
            mapper,
            valuation,
            width: self.width,
        };
        let register = builder.build(&journal);
        if register.is_empty() {
            eprintln!(
                "warning: no bookings for {} or the accounts below it in this period",
                self.account
            );
        }
        // A valued amount is money, and its scale is an artifact of the
        // price it was multiplied by rather than a precision worth showing.
        // A quantity is as the journal wrote it.
        let round = self.round.unwrap_or_else(|| match valuation {
            Some(_) => 2,
            None => register.scale(),
        });
        let renderer = TextRenderer::new(register.to_table(), round);
        let mut lock = stdout().lock();
        renderer.render(lock.borrow_mut())?;
        lock.flush()?;
        Ok(())
    }
}
