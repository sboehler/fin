use super::Account;
use super::Commodity;
use chrono::prelude::NaiveDate;
use rust_decimal::prelude::Decimal;
use std::fmt;
use std::fmt::Display;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct Assertion {
    pub date: NaiveDate,
    pub account: Arc<Account>,
    pub balance: Decimal,
    pub commodity: Arc<Commodity>,
}

impl Assertion {
    pub fn new(
        date: NaiveDate,
        account: Arc<Account>,
        balance: Decimal,
        commodity: Arc<Commodity>,
    ) -> Assertion {
        Assertion {
            date,
            account,
            balance,
            commodity,
        }
    }
}

impl Display for Assertion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{} balance {} {} {}",
            self.date.format("%Y-%m-%d"),
            self.account,
            self.balance,
            self.commodity
        )
    }
}
