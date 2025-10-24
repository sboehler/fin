use std::collections::HashSet;
use std::ops::{Deref, DerefMut, Neg};
use std::{collections::BTreeMap, rc::Rc};

use chrono::NaiveDate;
use rust_decimal::Decimal;

use super::entities::{
    AccountID, Assertion, Booking, Close, CommodityID, Open, Partition, Period, Positions, Price,
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
    registry: Rc<Registry>,
    days: BTreeMap<NaiveDate, Day>,
}

impl Default for Journal {
    fn default() -> Self {
        Self {
            registry: Rc::new(Registry::new()),
            days: BTreeMap::new(),
        }
    }
}

impl Journal {
    pub fn new(registry: Rc<Registry>, days: BTreeMap<NaiveDate, Day>) -> Self {
        Self { registry, days }
    }

    pub fn day(&mut self, date: NaiveDate) -> &mut Day {
        self.days.entry(date).or_insert_with(|| Day::new(date))
    }

    pub fn min_transaction_date(&self) -> Option<NaiveDate> {
        self.days
            .values()
            .find(|d| !d.transactions.is_empty())
            .map(|d| d.date)
    }

    pub fn max_transaction_date(&self) -> Option<NaiveDate> {
        self.days
            .values()
            .rfind(|d| !d.transactions.is_empty())
            .map(|d| d.date)
    }

    pub fn entire_period(&self) -> Option<Period> {
        self.days
            .keys()
            .next()
            .and_then(|t0| self.days.keys().last().map(|t1| Period(*t0, *t1)))
    }

    pub fn check(&self) -> std::result::Result<(), JournalError> {
        let mut quantities = Positions::default();
        let mut accounts = HashSet::new();

        for day in self.days.values() {
            for o in &day.openings {
                if !accounts.insert(o.account) {
                    return Err(JournalError::AccountAlreadyOpen {
                        open: Box::new(o.clone()),
                        registry: self.registry.clone(),
                    });
                }
            }
            for t in &day.transactions {
                for b in &t.bookings {
                    if !accounts.contains(&b.account) {
                        return Err(JournalError::TransactionAccountNotOpen {
                            transaction: Box::new(t.clone()),
                            account: b.account,
                            registry: self.registry.clone(),
                        });
                    }
                    quantities.insert_or_add((b.account, b.commodity), &b.quantity);
                }
            }
            for a in &day.assertions {
                if !accounts.contains(&a.account) {
                    return Err(JournalError::AssertionAccountNotOpen {
                        assertion: Box::new(a.clone()),
                        registry: self.registry.clone(),
                    });
                }
                let balance = quantities
                    .get(&(a.account, a.commodity))
                    .copied()
                    .unwrap_or_default();
                if balance != a.balance {
                    return Err(JournalError::AssertionIncorrectBalance {
                        assertion: Box::new(a.clone()),
                        actual: balance,
                        registry: self.registry.clone(),
                    });
                }
            }
            for c in &day.closings {
                for (pos, qty) in quantities.iter() {
                    if pos.0 == c.account && !qty.is_zero() {
                        return Err(JournalError::CloseNonzeroBalance {
                            close: Box::new(c.clone()),
                            commodity: pos.1,
                            balance: *qty,
                            registry: self.registry.clone(),
                        });
                    }
                }
                accounts.remove(&c.account);
            }
        }
        Ok(())
    }

    pub fn process(&mut self, valuation: Option<CommodityID>) -> Result<(), ModelError> {
        let mut prices = Prices::default();
        let mut quantities = Positions::default();
        let mut values = Positions::default();

        for date in self.entire_period().expect("journal is empty").dates() {
            let day = self.days.entry(date).or_insert_with(|| Day::new(date));
            for p in &day.prices {
                prices.insert(p);
            }
            let normalized_prices = valuation.map(|p| prices.normalize(p));
            Self::valuate_transactions(&self.registry, &mut day.transactions, &normalized_prices)?;
            day.gains = Self::compute_gains(
                self.registry.clone(),
                &normalized_prices,
                &quantities,
                &values,
                day.date,
            )?;
            Self::update_quantities(&day.transactions, &mut quantities);
            Self::update_values(&day.transactions, &mut values);
            Self::update_values(&day.gains, &mut values);
        }
        Ok(())
    }

    fn update_quantities(
        transactions: &[Transaction],
        quantities: &mut Positions<(AccountID, CommodityID), Decimal>,
    ) {
        transactions
            .iter()
            .flat_map(|t| t.bookings.iter())
            .for_each(|b| quantities.insert_or_add((b.account, b.commodity), &b.quantity));
    }

    fn update_values(
        transactions: &[Transaction],
        values: &mut Positions<(AccountID, CommodityID), Decimal>,
    ) {
        transactions
            .iter()
            .flat_map(|t| t.bookings.iter())
            .for_each(|b| {
                values.insert_or_add((b.account, b.commodity), &b.value.unwrap_or_default())
            });
    }

    fn valuate_transactions(
        registry: &Rc<Registry>,
        transactions: &mut Vec<Transaction>,
        normalized_prices: &Option<NormalizedPrices>,
    ) -> Result<(), ModelError> {
        for t in transactions {
            for b in &mut t.bookings {
                b.value = normalized_prices
                    .as_ref()
                    .map(|p| p.valuate(registry, &b.quantity, b.commodity))
                    .transpose()?;
            }
        }
        Ok(())
    }

    fn compute_gains(
        registry: Rc<Registry>,
        normalized_prices: &Option<NormalizedPrices>,
        quantities: &Positions<(AccountID, CommodityID), Decimal>,
        values: &Positions<(AccountID, CommodityID), Decimal>,
        date: NaiveDate,
    ) -> Result<Vec<Transaction>, ModelError> {
        let Some(normalized_prices) = normalized_prices.as_ref() else {
            return Ok(Vec::new());
        };
        let mut gains = Vec::new();

        for ((account, commodity), qty) in quantities.iter() {
            if !account.account_type.is_al() {
                continue;
            }
            let previous_value = values
                .get(&(*account, *commodity))
                .copied()
                .unwrap_or_default();
            if qty.is_zero() && previous_value.is_zero() {
                continue;
            }
            let current_value = normalized_prices.valuate(&registry, qty, *commodity)?;
            let gain = current_value - previous_value;
            if gain.is_zero() {
                continue;
            }
            gains.push(Transaction {
                date,
                loc: None,
                description: format!(
                    "Adjust value of {} in account {}",
                    registry.commodity_name(*commodity),
                    registry.account_name(*account)
                )
                .into(),
                bookings: Booking::create(
                    registry.valuation_account_for(*account),
                    *account,
                    Decimal::ZERO,
                    *commodity,
                    Some(gain),
                ),
                targets: Some(vec![*commodity]),
            });
        }
        Ok(gains)
    }
}

impl Journal {
    pub fn query<'a>(&'a self, part: &'a Partition) -> impl Iterator<Item = Entry> + 'a {
        self.days
            .values()
            .filter(|day| part.contains(day.date))
            .flat_map(|day| day.transactions.iter().chain(day.gains.iter()))
            .flat_map(|t| {
                t.bookings.iter().map(|b| Entry {
                    date: t.date,
                    description: t.description.clone(),
                    account: b.account,
                    other: b.other,
                    commodity: b.commodity,
                    quantity: b.quantity,
                    value: b.value,
                })
            })
    }

    pub fn registry(&self) -> &Rc<Registry> {
        &self.registry
    }
}

impl Deref for Journal {
    type Target = BTreeMap<NaiveDate, Day>;

    fn deref(&self) -> &Self::Target {
        &self.days
    }
}

impl DerefMut for Journal {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.days
    }
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub date: NaiveDate,
    pub account: AccountID,
    pub other: AccountID,
    pub commodity: CommodityID,
    pub description: Rc<String>,
    pub quantity: Decimal,
    pub value: Option<Decimal>,
}

pub struct Closer {
    dates: Vec<NaiveDate>,
    close: bool,
    current: usize,
    quantities: Positions<(AccountID, CommodityID), Decimal>,
    values: Positions<(AccountID, CommodityID), Decimal>,

    equity: AccountID,
}

impl Closer {
    pub fn new(dates: Vec<NaiveDate>, equity: AccountID, close: bool) -> Self {
        Closer {
            dates,
            close,
            equity,
            quantities: Default::default(),
            values: Default::default(),
            current: 0,
        }
    }

    pub fn process(&mut self, r: Entry) -> Vec<Entry> {
        if !self.close {
            return vec![r];
        }
        let mut res = Vec::new();
        if self.current < self.dates.len() {
            if r.date >= self.dates[self.current] {
                let closing_date = self.dates[self.current];
                res.extend(
                    self.quantities
                        .iter()
                        .map(|(k @ (account, commodity), quantity)| Entry {
                            date: closing_date,
                            description: Rc::new("".into()),
                            account: *account,
                            other: self.equity,
                            commodity: *commodity,
                            quantity: -*quantity,
                            value: self.values.get(k).copied().map(Neg::neg),
                        }),
                );
                res.extend(
                    self.quantities
                        .iter()
                        .map(|(k @ (account, commodity), quantity)| Entry {
                            date: closing_date,
                            description: Rc::new("".into()),
                            account: self.equity,
                            other: *account,
                            commodity: *commodity,
                            quantity: *quantity,
                            value: self.values.get(k).copied(),
                        }),
                );

                self.current += 1;
                self.quantities.clear();
                self.values.clear();
            }
            if r.account.account_type.is_ie() {
                self.quantities
                    .insert_or_add((r.account, r.commodity), &r.quantity);
                if let Some(value) = &r.value {
                    self.values.insert_or_add((r.account, r.commodity), value);
                }
            };
        }
        res.push(r);
        res
    }
}

// pub struct Filter {
//     period: Option<Period>,
//     account: Option<RegexSet>,
//     commodity: Option<RegexSet>,
// }
// impl Filter {
//     pub fn process(&self, r: Row) -> bool {
//         self.period
//             .map(|period| period.contains(r.date))
//             .unwrap_or(true)
//             && self
//                 .account
//                 .as_ref()
//                 .map(|account| account.is_match(&r.account.name) || account.is_match(&r.other.name))
//                 .unwrap_or(true)
//             && self
//                 .commodity
//                 .as_ref()
//                 .map(|commodity| commodity.is_match(&r.commodity.name))
//                 .unwrap_or(true)
//     }
// }
