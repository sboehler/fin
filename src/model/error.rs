use std::{fmt::Display, rc::Rc};

use chrono::NaiveDate;
use thiserror::Error;

use super::entities::Commodity;

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
