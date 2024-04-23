use super::{Account, Commodity, Lot};
use rust_decimal::prelude::Decimal;
use std::fmt;
use std::fmt::Display;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub struct Posting {
    pub account: Arc<Account>,
    pub other: Arc<Account>,
    pub commodity: Arc<Commodity>,
    pub amount: Decimal,
    pub value: Decimal,
    pub lot: Option<Lot>,
    pub targets: Option<Vec<Arc<Commodity>>>,
}

impl Display for Posting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rev = self.amount < Decimal::ZERO
            || self.amount == Decimal::ZERO && self.value < Decimal::ZERO;
        let (credit, debit) = if rev {
            (&self.account, &self.other)
        } else {
            (&self.other, &self.account)
        };
        write!(f, "{} {} {} {}", credit, debit, self.amount, self.commodity)?;
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

pub struct PostingBuilder {
    pub credit: Arc<Account>,
    pub debit: Arc<Account>,
    pub commodity: Arc<Commodity>,
    pub amount: Decimal,
    pub value: Decimal,
    pub lot: Option<Lot>,
    pub targets: Option<Vec<Arc<Commodity>>>,
}

impl PostingBuilder {
    pub fn build(self) -> Vec<Posting> {
        let rev = self.amount < Decimal::ZERO
            || self.amount == Decimal::ZERO && self.value < Decimal::ZERO;
        let credit = if rev {
            &self.debit
        } else {
            &self.credit
        };
        let debit = if rev {
            &self.credit
        } else {
            &self.debit
        };
        let amount = if rev {
            -self.amount
        } else {
            self.amount
        };
        let value = if rev {
            -self.value
        } else {
            self.value
        };

        return vec![
            Posting {
                account: credit.clone(),
                other: debit.clone(),
                commodity: self.commodity.clone(),
                amount: -amount,
                value: -value,
                targets: self.targets.clone(),
                lot: self.lot.clone(),
            },
            Posting {
                account: debit.clone(),
                other: credit.clone(),
                commodity: self.commodity.clone(),
                amount: amount,
                value: value,
                targets: self.targets.clone(),
                lot: self.lot.clone(),
            },
        ];
    }
}
