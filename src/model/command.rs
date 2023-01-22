use chrono::NaiveDate;

use super::{Assertion, Close, Open, Price, Transaction, Value};
use std::fmt;
use std::fmt::Display;

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd)]
pub enum Command {
    Open(Open),
    Price(Price),
    Trx(Transaction),
    Value(Value),
    Assertion(Assertion),
    Close(Close),
}

impl Command {
    pub fn date(&self) -> NaiveDate {
        match self {
            Command::Open(o) => o.date,
            Command::Price(p) => p.date,
            Command::Trx(t) => t.date,
            Command::Value(v) => v.date,
            Command::Assertion(a) => a.date,
            Command::Close(c) => c.date,
        }
    }
}

impl Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::Open(o) => write!(f, "{}", o),
            Command::Price(p) => write!(f, "{}", p),
            Command::Trx(t) => write!(f, "{}", t),
            Command::Value(v) => write!(f, "{}", v),
            Command::Assertion(a) => write!(f, "{}", a),
            Command::Close(c) => write!(f, "{}", c),
        }
    }
}
