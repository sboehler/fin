use super::Commodity;
use chrono::prelude::NaiveDate;
use rust_decimal::prelude::Decimal;
use std::fmt;
use std::fmt::Display;

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct Lot {
    price: Decimal,
    commodity: Commodity,
    date: Option<NaiveDate>,
    label: Option<String>,
}

impl Lot {
    pub fn new(
        price: Decimal,
        commodity: Commodity,
        date: Option<NaiveDate>,
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
        write!(f, "{{ {} {}", self.price, self.commodity)?;
        if let Some(d) = self.date {
            write!(f, ", {}", d)?;
        }
        if let Some(l) = &self.label {
            write!(f, ", {}", l)?
        }
        write!(f, "}}")
    }
}
