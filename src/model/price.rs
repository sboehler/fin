use super::Commodity;
use chrono::prelude::NaiveDate;
use rust_decimal::prelude::Decimal;
use std::fmt;
use std::fmt::Display;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Price {
    pub date: NaiveDate,
    pub price: Decimal,
    pub source: Commodity,
    pub target: Commodity,
}

impl Price {
    pub fn new(date: NaiveDate, price: Decimal, target: Commodity, source: Commodity) -> Price {
        Price {
            date,
            price,
            source,
            target,
        }
    }
}

impl Display for Price {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{} price {} {} {}",
            self.date.format("%Y-%m-%d"),
            self.source,
            self.price,
            self.target
        )
    }
}
