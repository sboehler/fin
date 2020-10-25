use chrono::prelude::NaiveDate;
use rust_decimal::prelude::{Decimal, Zero};
use std::fmt;
use std::fmt::Display;
use std::result::Result;

use super::{Account, Commodity, Posting, Tag};

#[derive(Debug, Clone, PartialEq)]
pub struct Transaction {
    pub date: NaiveDate,
    pub description: String,
    pub tags: Vec<Tag>,
    pub postings: Vec<Posting>,
}

impl Transaction {
    pub fn new(
        d: NaiveDate,
        desc: String,
        tags: Vec<Tag>,
        mut postings: Vec<Posting>,
        account: Option<Account>,
    ) -> Result<Transaction, String> {
        let mut amounts: Vec<(Commodity, Decimal)> = Vec::with_capacity(postings.len());
        for p in postings.iter() {
            match amounts.iter_mut().find(|c| c.0 == p.commodity) {
                None => amounts.push((p.commodity.clone(), p.amount)),
                Some(e) => e.1 += p.amount,
            };
        }
        for amt in amounts.iter() {
            if amt.1.is_zero() {
                continue;
            }
            match &account {
                None => return Err("Transaction is not balanced".into()),
                Some(a) => postings.push(Posting {
                    account: a.clone(),
                    commodity: amt.0.clone(),
                    amount: -amt.1,
                    lot: None,
                    tag: None,
                }),
            }
        }
        Ok(Transaction {
            date: d,
            description: desc,
            tags,
            postings,
        })
    }
}

impl Display for Transaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} \"{}\"",
            self.date.format("%Y-%m-%d"),
            self.description
        )?;
        for t in &self.tags {
            write!(f, " {}", t)?
        }
        for posting in &self.postings {
            writeln!(f)?;
            write!(f, "{}", posting)?;
        }
        Ok(())
    }
}
