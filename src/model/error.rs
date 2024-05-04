use std::{error::Error, fmt::Display};

#[derive(Debug, Eq, PartialEq)]
pub enum ModelError {
    InvalidAccountType,
    InvalidCommodityName,
    InvalidAccountName,
}

impl Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidAccountType => write!(f, "invalid account type"),
            Self::InvalidCommodityName => write!(f, "invalid commodity name"),
            Self::InvalidAccountName => write!(f, "invalid account name"),
        }
    }
}

impl Error for ModelError {}
