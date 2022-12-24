use core::fmt;
use std::fmt::Display;

use chrono::NaiveDate;

#[derive(Clone, Eq, PartialEq, Debug, Ord, PartialOrd)]
pub enum Interval {
    Once,
    Daily,
    Weekly,
    Monthly,
    Quarterly,
    Yearly,
}

impl Display for Interval {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Once => write!(f, "once"),
            Self::Daily => write!(f, "daily"),
            Self::Weekly => write!(f, "weekly"),
            Self::Monthly => write!(f, "monthly"),
            Self::Quarterly => write!(f, "quarterly"),
            Self::Yearly => write!(f, "yearly"),
        }
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Ord, PartialOrd)]
pub struct Period {
    pub start: NaiveDate,
    pub end: NaiveDate,
}
