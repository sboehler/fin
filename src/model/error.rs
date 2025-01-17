use std::fmt::Display;

use chrono::NaiveDate;
use rust_decimal::Decimal;
use thiserror::Error;

use crate::syntax::cst::Rng;

use super::entities::{Assertion, Close, Open, Transaction};

#[derive(Error, Debug, Eq, PartialEq)]
pub enum ModelError {
    InvalidAccountType(String),
    InvalidCommodityName(String),
    InvalidAccountName(String),
    NoPriceFound {
        date: NaiveDate,
        commodity_name: String,
        target_name: String,
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
                commodity_name: commodity,
                target_name: target,
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
        open: Box<Open>,
        account_name: String,
    },
    TransactionAccountNotOpen {
        transaction: Box<Transaction>,
        account_name: String,
    },
    AssertionAccountNotOpen {
        assertion: Box<Assertion>,
        account_name: String,
    },
    AssertionIncorrectBalance {
        assertion: Box<Assertion>,
        actual: Decimal,
        account_name: String,
        commodity_name: String,
    },
    CloseNonzeroBalance {
        close: Box<Close>,
        commodity_name: String,
        balance: Decimal,
        account_name: String,
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
            JournalError::AccountAlreadyOpen { open, account_name } => {
                writeln!(
                    f,
                    "Error: open directive on {date}: account {account} is already open.",
                    date = open.date,
                    account = account_name,
                )?;
                Self::write_context(&open.rng, f)?
            }
            JournalError::TransactionAccountNotOpen {
                transaction,
                account_name,
            } => {
                writeln!(
                    f,
                    "Error: transaction directive on {date}: account {account_name} is not open.",
                    date = transaction.date,
                )?;
                Self::write_context(&transaction.rng, f)?
            }
            JournalError::AssertionAccountNotOpen {
                assertion,
                account_name,
            } => {
                writeln!(
                    f,
                    "Error: balance directive on {date}: account {account} is not open.",
                    account = account_name,
                    date = assertion.date,
                )?;
                Self::write_context(&assertion.rng, f)?
            }
            JournalError::AssertionIncorrectBalance {
                assertion,
                actual,
                account_name,
                commodity_name,
            } => {
                writeln!(
                    f,
                    "Error: balance directive on {date}: account {account_name} has balance {actual} {commodity_name}, want {balance} {commodity_name}.",
                    balance = assertion.balance,
                    date = assertion.date,
                )?;
                Self::write_context(&assertion.rng, f)?
            }
            JournalError::CloseNonzeroBalance {
                close,
                commodity_name,
                balance,
                account_name,
            } => {
                writeln!(
                    f,
                    "Error: close directive on {date}: account {account_name} still has a balance of {balance} {commodity_name}, want zero.",
                    date = close.date,
                )?;
                Self::write_context(&close.rng, f)?
            }
        }
        Ok(())
    }
}
