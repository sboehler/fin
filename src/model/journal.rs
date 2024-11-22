use std::collections::{HashMap, HashSet};
use std::{collections::BTreeMap, rc::Rc};

use chrono::NaiveDate;
use rust_decimal::Decimal;

use super::entities::{
    start_of, Account, Assertion, Booking, Close, Commodity, Interval, Open, Period, Price,
    Transaction,
};
use super::error::{JournalError, ModelError};
use super::prices::{NormalizedPrices, Prices};
use super::registry::Registry;

pub struct Day {
    pub date: NaiveDate,
    pub prices: Vec<Price>,
    pub assertions: Vec<Assertion>,
    pub openings: Vec<Open>,
    pub transactions: Vec<Transaction>,

    pub gains: Vec<Transaction>,
    pub closings: Vec<Close>,
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
        }
    }
}

pub struct Journal {
    pub registry: Rc<Registry>,
    pub days: BTreeMap<NaiveDate, Day>,

    pub valuation: Option<Rc<Commodity>>,
    pub closing: Option<Interval>,
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
            valuation: None,
            closing: None,
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

    pub fn period(&self) -> Option<Period> {
        self.days
            .keys()
            .next()
            .and_then(|t0| self.days.keys().last().map(|t1| Period(*t0, *t1)))
    }

    pub fn check(&self) -> std::result::Result<(), JournalError> {
        let mut quantities = Positions::default();
        let mut accounts = HashSet::new();

        for day in self.days.values() {
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

    pub fn process(
        &mut self,
        valuation: Option<&Rc<Commodity>>,
        close: Option<Interval>,
    ) -> Result<(), ModelError> {
        let mut prices = Prices::default();
        let mut quantities = Positions::default();
        let mut prev_normalized_prices = None;

        self.valuation = valuation.cloned();
        self.closing = close;

        for date in self.period().expect("journal is empty").dates() {
            let closings = close
                .filter(|&interval| date == start_of(date, interval).unwrap())
                .map(|_| Vec::new())
                .unwrap_or_default();

            if let Some(day) = self.days.get_mut(&date) {
                day.prices.iter().for_each(|p| prices.insert(p));

                let normalized_prices = valuation.map(|p| prices.normalize(p));
                let credit = self.registry.account("Income:Valuation")?;

                Self::valuate_transactions(&mut day.transactions, &normalized_prices)?;

                day.gains = Self::compute_gains(
                    &normalized_prices,
                    &quantities,
                    &prev_normalized_prices,
                    day.date,
                    credit,
                )?;
                Self::update_quantities(&day.transactions, &mut quantities);
                prev_normalized_prices = normalized_prices;
                day.closings = closings
            } else {
                let mut day = Day::new(date);
                day.closings = closings;
                self.days.insert(date, day);
            }
        }
        Ok(())
    }

    fn update_quantities(
        transactions: &[Transaction],
        quantities: &mut std::collections::HashMap<(Rc<Account>, Rc<Commodity>), Decimal>,
    ) {
        transactions
            .iter()
            .flat_map(|t| t.bookings.iter())
            .for_each(|b| {
                quantities
                    .entry((Rc::clone(&b.account), Rc::clone(&b.commodity)))
                    .and_modify(|q| *q += b.quantity)
                    .or_insert(b.quantity);
            });
    }

    fn valuate_transactions(
        transactions: &mut Vec<Transaction>,
        normalized_prices: &Option<NormalizedPrices>,
    ) -> Result<(), ModelError> {
        for t in transactions {
            for b in &mut t.bookings {
                b.value = normalized_prices
                    .as_ref()
                    .map(|p| p.valuate(&b.quantity, &b.commodity))
                    .transpose()?
                    .unwrap_or(Decimal::ZERO);
            }
        }
        Ok(())
    }

    fn compute_gains(
        normalized_prices: &Option<NormalizedPrices>,
        quantities: &std::collections::HashMap<(Rc<Account>, Rc<Commodity>), Decimal>,
        prev_normalized_prices: &Option<NormalizedPrices>,
        date: NaiveDate,
        credit: Rc<Account>,
    ) -> Result<Vec<Transaction>, ModelError> {
        let gains = normalized_prices
            .as_ref()
            .map(|np| {
                Ok(quantities
                    .iter()
                    .map(|((account, commodity), qty)| {
                        if qty.is_zero() || !account.account_type.is_al() {
                            return Ok(None);
                        }
                        let v_prev = prev_normalized_prices
                            .as_ref()
                            .unwrap()
                            .valuate(qty, commodity)?;
                        let v_cur = np.valuate(qty, commodity)?;
                        let gain = v_cur - v_prev;
                        if gain.is_zero() {
                            return Ok(None);
                        }
                        Ok(Some(Transaction {
                            date,
                            rng: None,
                            description: format!(
                                "Adjust value of {} in account {}",
                                commodity.name, account.name
                            )
                            .into(),
                            bookings: Booking::create(
                                credit.clone(),
                                account.clone(),
                                Decimal::ZERO,
                                commodity.clone(),
                                gain,
                            ),
                            targets: Some(vec![commodity.clone()]),
                        }))
                    })
                    .collect::<Result<Vec<Option<Transaction>>, ModelError>>()?
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>())
            })
            .transpose()?
            .unwrap_or_default();
        Ok(gains)
    }
}

impl Journal {
    pub fn query(&self) -> impl Iterator<Item = Row> + '_ {
        return self.days.values().flat_map(|day| {
            day.transactions
                .iter()
                .chain(day.gains.iter())
                .flat_map(|t| {
                    t.bookings.iter().map(|b| Row {
                        date: t.date,
                        description: t.description.clone(),
                        account: b.account.clone(),
                        other: b.other.clone(),
                        commodity: b.commodity.clone(),
                        valuation: self.valuation.clone(),
                    })
                })
        });
    }
}

pub struct Row {
    pub date: NaiveDate,
    pub account: Rc<Account>,
    pub other: Rc<Account>,
    pub commodity: Rc<Commodity>,
    pub valuation: Option<Rc<Commodity>>,
    pub description: Rc<String>,
}
