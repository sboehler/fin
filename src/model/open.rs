use super::Account;
use chrono::prelude::NaiveDate;
use std::fmt;
use std::fmt::Display;

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct Open {
    pub date: NaiveDate,
    pub account: Account,
}

impl Display for Open {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{} open {}", self.date.format("%Y-%m-%d"), self.account)
    }
}
