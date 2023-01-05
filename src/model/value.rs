use super::{Account, Commodity};
use chrono::prelude::NaiveDate;
use rust_decimal::prelude::Decimal;
use std::fmt;
use std::fmt::Display;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct Value {
    pub date: NaiveDate,
    pub amount: Decimal,
    pub account: Arc<Account>,
    pub commodity: Commodity,
}

impl Value {
    pub fn new(
        date: NaiveDate,
        account: Arc<Account>,
        amount: Decimal,
        commodity: Commodity,
    ) -> Self {
        Value {
            date,
            account,
            amount,
            commodity,
        }
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{} value {} {} {}",
            self.date.format("%Y-%m-%d"),
            self.account,
            self.amount,
            self.commodity
        )
    }
}
