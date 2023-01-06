use super::Commodity;
use chrono::prelude::NaiveDate;
use rust_decimal::prelude::Decimal;
use std::fmt;
use std::fmt::Display;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct Price {
    pub date: NaiveDate,
    pub price: Decimal,
    pub source: Arc<Commodity>,
    pub target: Arc<Commodity>,
}

impl Price {
    pub fn new(
        date: NaiveDate,
        price: Decimal,
        target: Arc<Commodity>,
        source: Arc<Commodity>,
    ) -> Price {
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
