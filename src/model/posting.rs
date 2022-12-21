use super::{Account, Commodity, Lot, Tag};
use rust_decimal::prelude::Decimal;
use std::fmt;
use std::fmt::Display;

#[derive(Debug, Clone, PartialEq)]
pub struct Posting {
    pub account: Account,
    pub commodity: Commodity,
    pub amount: Decimal,
    pub lot: Option<Lot>,
    pub tag: Option<Tag>,
}

impl Display for Posting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {}", self.account, self.amount, self.commodity)?;
        if let Some(l) = &self.lot {
            write!(f, " {}", l)?
        }
        if let Some(t) = &self.tag {
            write!(f, " {}", t)?
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Posting2 {
    pub credit: Account,
    pub debit: Account,
    pub commodity: Commodity,
    pub amount: Decimal,
    pub lot: Option<Lot>,
}

impl Display for Posting2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {} {}",
            self.credit, self.debit, self.amount, self.commodity
        )?;
        if let Some(l) = &self.lot {
            write!(f, " {}", l)?
        }
        writeln!(f)
    }
}
