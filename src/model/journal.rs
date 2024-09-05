use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};
use std::{collections::BTreeMap, rc::Rc};

use chrono::NaiveDate;
use rust_decimal::Decimal;

use super::entities::{Account, Assertion, Close, Commodity, Open, Price, Transaction};
use super::error::JournalError;
use super::prices::{NormalizedPrices, Prices};
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

    pub fn check(self: &Self) -> std::result::Result<(), JournalError> {
        let mut quantities = Positions::default();
        let mut accounts = HashSet::new();

        for day in self {
            day.openings.iter().try_for_each(|o| {
                if !accounts.insert(o.account.clone()) {
                    return Err(JournalError::AccountAlreadyOpen { open: o.clone() });
                }
                Ok(())
            })?;
            day.transactions.iter().try_for_each(|t| {
                t.bookings.iter().try_for_each(|b| {
                    if !accounts.contains(&b.account) {
                        return Err(JournalError::TransactionAccountNotOpen {
                            transaction: t.clone(),
                            account: b.account.clone(),
                        });
                    }
                    quantities
                        .entry((b.account.clone(), b.commodity.clone()))
                        .and_modify(|q| *q += b.quantity)
                        .or_insert(b.quantity);
                    Ok(())
                })
            })?;
            day.assertions.iter().try_for_each(|a| {
                if !accounts.contains(&a.account) {
                    return Err(JournalError::AssertionAccountNotOpen {
                        assertion: a.clone(),
                    });
                }
                let balance = quantities
                    .get(&(a.account.clone(), a.commodity.clone()))
                    .copied()
                    .unwrap_or_default();
                if balance != a.balance {
                    return Err(JournalError::AssertionIncorrectBalance {
                        assertion: a.clone(),
                        actual: balance,
                    });
                }
                Ok(())
            })?;
            day.closings.iter().try_for_each(|c| {
                for (pos, qty) in quantities.iter() {
                    if pos.0 == c.account && !qty.is_zero() {
                        return Err(JournalError::CloseNonzeroBalance {
                            close: c.clone(),
                            commodity: pos.1.clone(),
                            balance: *qty,
                        });
                    }
                }
                accounts.remove(&c.account);
                Ok(())
            })?;
        }
        Ok(())
    }

    pub fn compute_prices(
        self: &Self,
        valuation: &Rc<Commodity>,
    ) -> BTreeMap<NaiveDate, NormalizedPrices> {
        let mut prices = Prices::default();
        let mut res = BTreeMap::new();
        for day in self {
            day.prices.iter().for_each(|p| prices.insert(p));
            res.insert(day.date, prices.normalize(&valuation));
        }
        res
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
