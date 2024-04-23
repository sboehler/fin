use chrono::prelude::NaiveDate;
use rust_decimal::Decimal;
use std::fmt::Display;
use std::{fmt, sync::Arc};

use super::{Account, Interval, Period, Posting, Tag};

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct Transaction {
    pub date: NaiveDate,
    pub description: String,
    pub tags: Vec<Tag>,
    pub postings: Vec<Posting>,
    pub accrual: Option<Accrual>,
}

impl Transaction {
    pub fn new(
        d: NaiveDate,
        desc: String,
        tags: Vec<Tag>,
        postings: Vec<Posting>,
        accrual: Option<Accrual>,
    ) -> Transaction {
        Transaction {
            date: d,
            description: desc,
            tags,
            postings,
            accrual,
        }
    }
}

impl Display for Transaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref acc) = self.accrual {
            writeln!(
                f,
                "@accrue {interval} {start} {end} {account}",
                interval = acc.interval,
                start = acc.period.start,
                end = acc.period.end,
                account = acc.account
            )?;
        }
        write!(f, "{} \"{}\"", self.date.format("%Y-%m-%d"), self.description)?;
        for t in &self.tags {
            write!(f, " {}", t)?
        }
        writeln!(f)?;
        for posting in &self.postings {
            if posting.amount > Decimal::ZERO {
                write!(f, "{}", posting)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Accrual {
    pub interval: Interval,
    pub period: Period,
    pub account: Arc<Account>,
}
