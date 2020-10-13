use super::Account;
use chrono::prelude::NaiveDate;
use std::fmt;
use std::fmt::Display;

#[derive(Debug, Clone, PartialEq)]
pub struct Close {
    pub date: NaiveDate,
    pub account: Account,
}

impl Display for Close {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} close {}", self.date.format("%Y-%m-%d"), self.account)
    }
}
