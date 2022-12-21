use chrono::prelude::NaiveDate;
use std::fmt;
use std::fmt::Display;

use super::{Posting, Tag};

#[derive(Debug, Clone, PartialEq)]
pub struct Transaction {
    pub date: NaiveDate,
    pub description: String,
    pub tags: Vec<Tag>,
    pub postings: Vec<Posting>,
}

impl Transaction {
    pub fn new(d: NaiveDate, desc: String, tags: Vec<Tag>, postings: Vec<Posting>) -> Transaction {
        Transaction {
            date: d,
            description: desc,
            tags,
            postings,
        }
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
