use std::cell::OnceCell;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

use chrono::NaiveDate;

use super::entities::{Assertion, Close, Open, Price, Transaction};
use super::prices::NormalizedPrices;
use super::registry::Registry;

#[derive(Debug)]
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

            normalized_prices: OnceCell::new(),
        }
    }
}

pub struct Journal {
    pub registry: Rc<RefCell<Registry>>,
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

    pub fn iter_mut(
        &mut self,
    ) -> std::collections::btree_map::ValuesMut<'_, chrono::NaiveDate, Day> {
        self.days.values_mut()
    }

    pub fn iter(&mut self) -> std::collections::btree_map::Values<'_, chrono::NaiveDate, Day> {
        self.days.values()
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

impl<'a> IntoIterator for &'a mut Journal {
    type Item = &'a mut Day;

    type IntoIter = std::collections::btree_map::ValuesMut<'a, chrono::NaiveDate, Day>;

    fn into_iter(self) -> Self::IntoIter {
        self.days.values_mut()
    }
}
