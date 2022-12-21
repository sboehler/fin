use super::{Assertion, Close, Open, Price, Transaction, Value};
use std::fmt;
use std::fmt::Display;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Open(Open),
    Close(Close),
    Trx(Transaction),
    Price(Price),
    Assertion(Assertion),
    Value(Value),
}

impl Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::Open(o) => write!(f, "{}", o),
            Command::Close(c) => write!(f, "{}", c),
            Command::Trx(t) => write!(f, "{}", t),
            Command::Price(p) => write!(f, "{}", p),
            Command::Assertion(a) => write!(f, "{}", a),
            Command::Value(v) => write!(f, "{}", v),
        }
    }
}
