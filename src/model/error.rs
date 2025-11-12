use std::{fmt::Display, rc::Rc};

use chrono::NaiveDate;
use rust_decimal::Decimal;
use thiserror::Error;

use crate::syntax::{error::SyntaxError, sourcefile::SourceFile};

use super::{
    entities::{AccountID, Assertion, Close, CommodityID, Open, SourceLoc, Transaction},
    registry::Registry,
};

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
    SyntaxError(SyntaxError, SourceFile),
}

impl Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidAccountType(s) => write!(f, "invalid account type: {s}"),
            Self::InvalidCommodityName(s) => write!(f, "invalid commodity name: {s}"),
            Self::InvalidAccountName(s) => write!(f, "invalid account name: {s}"),
            Self::NoPriceFound {
                date,
                commodity_name: commodity,
                target_name: target,
            } => {
                write!(
                    f,
                    "no price found for {commodity} on {date} in {target}"
                )
            }
            Self::SyntaxError(error, file) => error.full_error(f, file),
        }
    }
}

#[derive(Error, Debug)]
pub enum JournalError {
    AccountAlreadyOpen {
        open: Box<Open>,
        registry: Rc<Registry>,
    },
    TransactionAccountNotOpen {
        transaction: Box<Transaction>,
        account: AccountID,
        registry: Rc<Registry>,
    },
    AssertionAccountNotOpen {
        assertion: Box<Assertion>,
        registry: Rc<Registry>,
    },
    AssertionIncorrectBalance {
        assertion: Box<Assertion>,
        actual: Decimal,
        registry: Rc<Registry>,
    },
    CloseNonzeroBalance {
        close: Box<Close>,
        commodity: CommodityID,
        balance: Decimal,
        registry: Rc<Registry>,
    },
}

impl JournalError {
    pub fn write_context(
        location: &Option<SourceLoc>,
        f: &mut std::fmt::Formatter<'_>,
        registry: &Registry,
    ) -> std::fmt::Result {
        if let Some(loc) = location {
            let file = registry.source_file(loc.file);
            writeln!(f)?;
            if let Some(ref path) = file.path {
                write!(f, "Defined in file \"{}\", ", path.to_string_lossy())?;
            }
            let (line, col) = file.position(loc.start);
            writeln!(f, "line {line}, column {col}")?;
            writeln!(f)?;
            file.fmt_range(f, &loc.range())?;
        }
        Ok(())
    }
}

impl Display for JournalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JournalError::AccountAlreadyOpen { open, registry } => {
                writeln!(
                    f,
                    "Error: open directive on {date}: account {account} is already open.",
                    date = open.date,
                    account = registry.account_name(open.account),
                )?;
                Self::write_context(&open.loc, f, registry)?;
            }
            JournalError::TransactionAccountNotOpen {
                transaction,
                account,
                registry,
            } => {
                writeln!(
                    f,
                    "Error: transaction directive on {date}: account {account} is not open.",
                    date = transaction.date,
                    account = registry.account_name(*account),
                )?;
                Self::write_context(&transaction.loc, f, registry)?;
            }
            JournalError::AssertionAccountNotOpen {
                assertion,
                registry,
            } => {
                writeln!(
                    f,
                    "Error: balance directive on {date}: account {account} is not open.",
                    account = registry.account_name(assertion.account),
                    date = assertion.date,
                )?;
                Self::write_context(&assertion.loc, f, registry)?;
            }
            JournalError::AssertionIncorrectBalance {
                assertion,
                actual,
                registry,
            } => {
                writeln!(
                    f,
                    "Error: balance directive on {date}: account {account} has balance {actual} {commodity}, want {balance} {commodity}.",
                    balance = assertion.balance,
                    account = registry.account_name(assertion.account),
                    commodity = registry.commodity_name(assertion.commodity),
                    date = assertion.date,
                )?;
                Self::write_context(&assertion.loc, f, registry)?;
            }
            JournalError::CloseNonzeroBalance {
                close,
                commodity,
                balance,
                registry,
            } => {
                writeln!(
                    f,
                    "Error: close directive on {date}: account {account} still has a balance of {balance} {commodity}, want zero.",
                    date = close.date,
                    account = registry.account_name(close.account),
                    commodity = registry.commodity_name(*commodity),
                )?;
                Self::write_context(&close.loc, f, registry)?;
            }
        }
        Ok(())
    }
}
