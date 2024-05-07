use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

use chrono::NaiveDate;

use crate::syntax::file::FileError;

use super::{
    registry::Registry,
    {Assertion, Close, Open, Price, Transaction},
};

pub enum JournalError {
    FileError(FileError),
    IO(),
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Day {
    pub date: NaiveDate,
    pub prices: Vec<Price>,
    pub assertions: Vec<Assertion>,
    pub openings: Vec<Open>,
    pub transactions: Vec<Transaction>,
    pub closings: Vec<Close>,
}

impl Day {
    pub fn new(d: NaiveDate) -> Self {
        Day {
            date: d,
            prices: Vec::new(),
            assertions: Vec::new(),
            openings: Vec::new(),
            transactions: Vec::new(),
            closings: Vec::new(),
        }
    }
}

pub struct Journal {
    pub registry: Rc<RefCell<Registry>>,
    pub days: BTreeMap<NaiveDate, Day>,
}

impl Journal {
    pub fn new() -> Self {
        Journal {
            registry: Rc::new(RefCell::new(Registry::new())),
            days: BTreeMap::new(),
        }
    }

    pub fn day(&mut self, d: NaiveDate) -> &mut Day {
        self.days.entry(d).or_insert_with(|| Day::new(d))
    }

    pub fn min_date(&self) -> Option<NaiveDate> {
        self.days
            .values()
            .find(|d| !d.transactions.is_empty())
            .map(|d| d.date)
    }

    pub fn max_date(&self) -> Option<NaiveDate> {
        self.days
            .values()
            .rfind(|d| !d.transactions.is_empty())
            .map(|d| d.date)
    }
}
