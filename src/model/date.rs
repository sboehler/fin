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
        use super::Interval::*;
        match self {
            Once => write!(f, "once"),
            Daily => write!(f, "daily"),
            Weekly => write!(f, "weekly"),
            Monthly => write!(f, "monthly"),
            Quarterly => write!(f, "quarterly"),
            Yearly => write!(f, "yearly"),
        }
    }
}

#[derive(Clone, Eq, PartialEq, Debug, Ord, PartialOrd)]
pub struct Period {
    pub start: NaiveDate,
    pub end: NaiveDate,
}
