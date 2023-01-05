use super::Account;
use chrono::prelude::NaiveDate;
use std::fmt;
use std::fmt::Display;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct Close {
    pub date: NaiveDate,
    pub account: Arc<Account>,
}

impl Display for Close {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{} close {}", self.date.format("%Y-%m-%d"), self.account)
    }
}
