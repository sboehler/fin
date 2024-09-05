use std::cell::OnceCell;
use std::collections::HashMap;
use std::{collections::BTreeMap, rc::Rc};

use chrono::NaiveDate;
use rust_decimal::Decimal;

use super::entities::{Account, Assertion, Close, Commodity, Open, Price, Transaction};
use super::prices::NormalizedPrices;
use super::registry::Registry;

pub struct Day {
    pub date: NaiveDate,
    pub prices: Vec<Price>,
    pub assertions: Vec<Assertion>,
    pub openings: Vec<Open>,
    pub transactions: Vec<Transaction>,
    pub gains: OnceCell<Vec<Transaction>>,
    pub closings: Vec<Close>,

    pub normalized_prices: OnceCell<NormalizedPrices>,
}

pub type Positions = HashMap<(Rc<Account>, Rc<Commodity>), Decimal>;

impl Day {
    pub fn new(date: NaiveDate) -> Self {
        Day {
            date,
            prices: Vec::new(),
            assertions: Vec::new(),
            openings: Vec::new(),
            transactions: Vec::new(),
            gains: Default::default(),
            closings: Vec::new(),

            normalized_prices: Default::default(),
        }
    }
}

pub struct Journal {
    pub registry: Rc<Registry>,
    pub days: BTreeMap<NaiveDate, Day>,
}

impl Default for Journal {
    fn default() -> Self {
        Self::new()
    }
}

impl Journal {
    pub fn new() -> Self {
        Journal {
            registry: Rc::new(Registry::new()),
            days: BTreeMap::new(),
        }
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

impl IntoIterator for Journal {
    type Item = Day;

    type IntoIter = std::collections::btree_map::IntoValues<chrono::NaiveDate, Day>;
    fn into_iter(self) -> Self::IntoIter {
        self.days.into_values()
    }
}

impl<'a> IntoIterator for &'a Journal {
    type Item = &'a Day;

    type IntoIter = std::collections::btree_map::Values<'a, chrono::NaiveDate, Day>;

    fn into_iter(self) -> Self::IntoIter {
        self.days.values()
    }
}
