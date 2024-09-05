use std::{fmt::Display, rc::Rc};

use chrono::NaiveDate;
use rust_decimal::Decimal;
use thiserror::Error;

use crate::syntax::cst::Rng;

use super::entities::{Account, Assertion, Close, Commodity, Open, Transaction};

#[derive(Error, Debug, Eq, PartialEq)]
pub enum ModelError {
    InvalidAccountType(String),
    InvalidCommodityName(String),
    InvalidAccountName(String),
    NoPriceFound {
        date: NaiveDate,
        commodity: Rc<Commodity>,
        target: Rc<Commodity>,
    },
}

impl Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidAccountType(s) => write!(f, "invalid account type: {}", s),
            Self::InvalidCommodityName(s) => write!(f, "invalid commodity name: {}", s),
            Self::InvalidAccountName(s) => write!(f, "invalid account name: {}", s),
            Self::NoPriceFound {
                date,
                commodity,
                target,
            } => {
                write!(
                    f,
                    "no price found for {commodity} on {date} in {target}",
                    commodity = commodity,
                    date = date,
                    target = target
                )
            }
        }
    }
}

#[derive(Error, Eq, PartialEq, Debug)]
pub enum JournalError {
    AccountAlreadyOpen {
        open: Open,
    },
    TransactionAccountNotOpen {
        transaction: Transaction,
        account: Rc<Account>,
    },
    AssertionAccountNotOpen {
        assertion: Assertion,
    },
    AssertionIncorrectBalance {
        assertion: Assertion,
        actual: Decimal,
    },
    CloseNonzeroBalance {
        close: Close,
        commodity: Rc<Commodity>,
        balance: Decimal,
    },
}

impl JournalError {
    fn write_context(range: &Option<Rng>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(rng) = range {
            writeln!(f)?;
            if let Some(path) = &rng.file.path {
                write!(f, "Defined in file \"{}\", ", path.to_string_lossy())?;
            }
            let (line, col) = rng.file.position(rng.start);
            writeln!(f, "line {line}, column {col}")?;
            writeln!(f)?;
            writeln!(f, "{}", rng)?;
        }
        Ok(())
    }
}

impl Display for JournalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JournalError::AccountAlreadyOpen { open } => {
                writeln!(
                    f,
                    "Error: open directive on {date}: account {account} is already open.",
                    date = open.date,
                    account = open.account,
                )?;
                Self::write_context(&open.rng, f)?
            }
            JournalError::TransactionAccountNotOpen {
                transaction,
                account,
            } => {
                writeln!(
                    f,
                    "Error: transaction directive on {date}: account {account} is not open.",
                    date = transaction.date,
                )?;
                Self::write_context(&transaction.rng, f)?
            }
            JournalError::AssertionAccountNotOpen { assertion } => {
                writeln!(
                    f,
                    "Error: balance directive on {date}: account {account} is not open.",
                    account = assertion.account,
                    date = assertion.date,
                )?;
                Self::write_context(&assertion.rng, f)?
            }
            JournalError::AssertionIncorrectBalance { assertion, actual } => {
                writeln!(
                    f,
                    "Error: balance directive on {date}: account {account} has balance {actual} {commodity}, want {balance} {commodity}.",
                    account = assertion.account,
                    commodity = assertion.commodity,
                    balance = assertion.balance,
                    date = assertion.date,
                )?;
                Self::write_context(&assertion.rng, f)?
            }
            JournalError::CloseNonzeroBalance {
                close,
                commodity,
                balance,
            } => {
                writeln!(
                    f,
                    "Error: close directive on {date}: account {account} still has a balance of {balance} {commodity}, want zero.",
                    date = close.date,
                    account = close.account,
                )?;
                Self::write_context(&close.rng, f)?
            }
        }
        Ok(())
    }
}
