use super::{Account, Commodity, Lot};
use rust_decimal::prelude::Decimal;
use std::fmt;
use std::fmt::Display;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct Posting {
    pub credit: Arc<Account>,
    pub debit: Arc<Account>,
    pub commodity: Commodity,
    pub amount: Decimal,
    pub lot: Option<Lot>,
    pub targets: Option<Vec<Commodity>>,
}

impl Display for Posting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {} {}",
            self.credit, self.debit, self.amount, self.commodity
        )?;
        if let Some(l) = &self.lot {
            write!(f, " {}", l)?
        }
        if let Some(ts) = &self.targets {
            write!(f, " (")?;
            for t in ts.iter().enumerate() {
                write!(f, "{}", t.1)?;
                if t.0 < ts.len() - 1 {
                    write!(f, ",")?;
                }
            }
            write!(f, ")")?;
        }
        writeln!(f)
    }
}
