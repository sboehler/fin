use super::Commodity;
use chrono::prelude::NaiveDate;
use rust_decimal::prelude::Decimal;
use std::fmt;
use std::fmt::Display;

#[derive(Debug, Clone, PartialEq)]
pub struct Lot {
    price: Decimal,
    commodity: Commodity,
    date: NaiveDate,
    label: Option<String>,
}

impl Lot {
    pub fn new(
        price: Decimal,
        commodity: Commodity,
        date: NaiveDate,
        label: Option<String>,
    ) -> Self {
        Self {
            price,
            commodity,
            date,
            label,
        }
    }
}

impl Display for Lot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{ {} {}, {}", self.price, self.commodity, self.date)?;
        if let Some(l) = &self.label {
            write!(f, ", {}", l)?
        }
        write!(f, "}}")
    }
}
