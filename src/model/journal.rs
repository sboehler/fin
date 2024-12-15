use std::collections::HashSet;
use std::{collections::BTreeMap, rc::Rc};

use chrono::NaiveDate;
use rust_decimal::Decimal;

use super::entities::{
    AccountID, Assertion, Booking, Close, CommodityID, Interval, Open, Period, Positions, Price,
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
    pub registry: Rc<Registry>,
    pub days: BTreeMap<NaiveDate, Day>,

    pub valuation: Option<CommodityID>,
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
                if !accounts.insert(o.account) {
                    return Err(JournalError::AccountAlreadyOpen {
                        open: Box::new(o.clone()),
                        account_name: self.registry.account_name(o.account),
                    });
                }
                Ok(())
            })?;
            day.transactions.iter().try_for_each(|t| {
                t.bookings.iter().try_for_each(|b| {
                    if !accounts.contains(&b.account) {
                        return Err(JournalError::TransactionAccountNotOpen {
                            transaction: Box::new(t.clone()),
                            account_name: self.registry.account_name(b.account),
                        });
                    }
                    quantities.add(&(b.account, b.commodity), &b.quantity);
                    Ok(())
                })
            })?;
            day.assertions.iter().try_for_each(|a| {
                if !accounts.contains(&a.account) {
                    return Err(JournalError::AssertionAccountNotOpen {
                        assertion: Box::new(a.clone()),
                        account_name: self.registry.account_name(a.account),
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
                        account_name: self.registry.account_name(a.account),
                        commodity_name: self.registry.commodity_name(a.commodity),
                    });
                }
                Ok(())
            })?;
            day.closings.iter().try_for_each(|c| {
                for (pos, qty) in quantities.iter() {
                    if pos.0 == c.account && !qty.is_zero() {
                        return Err(JournalError::CloseNonzeroBalance {
                            close: Box::new(c.clone()),
                            commodity_name: self.registry.commodity_name(pos.1),
                            balance: *qty,
                            account_name: self.registry.account_name(c.account),
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
        valuation: Option<&CommodityID>,
        close: Option<Interval>,
    ) -> Result<(), ModelError> {
        let mut prices = Prices::default();
        let mut quantities = Positions::default();
        let mut prev_normalized_prices = None;

        self.valuation = valuation.cloned();
        self.closing = close;

        for date in self.period().expect("journal is empty").dates() {
            let closings = close
                .filter(|&interval| date == interval.start_of(date).unwrap())
                .map(|_| Vec::new())
                .unwrap_or_default();

            if let Some(day) = self.days.get_mut(&date) {
                day.prices.iter().for_each(|p| prices.insert(p));

                let normalized_prices = valuation.map(|p| prices.normalize(p));
                let credit = self.registry.account_id("Income:Valuation")?;

                Self::valuate_transactions(
                    &self.registry,
                    &mut day.transactions,
                    &normalized_prices,
                )?;

                day.gains = Self::compute_gains(
                    self.registry.clone(),
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
        quantities: &mut Positions<(AccountID, CommodityID), Decimal>,
    ) {
        transactions
            .iter()
            .flat_map(|t| t.bookings.iter())
            .for_each(|b| quantities.add(&(b.account, b.commodity), &b.quantity));
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
                    .map(|p| p.valuate(registry, &b.quantity, &b.commodity))
                    .transpose()?;
            }
        }
        Ok(())
    }

    fn compute_gains(
        registry: Rc<Registry>,
        normalized_prices: &Option<NormalizedPrices>,
        quantities: &Positions<(AccountID, CommodityID), Decimal>,
        prev_normalized_prices: &Option<NormalizedPrices>,
        date: NaiveDate,
        credit: AccountID,
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
                            .valuate(&registry, qty, commodity)?;
                        let v_cur = np.valuate(&registry, qty, commodity)?;
                        let gain = v_cur - v_prev;
                        if gain.is_zero() {
                            return Ok(None);
                        }
                        Ok(Some(Transaction {
                            date,
                            rng: None,
                            description: format!(
                                "Adjust value of {} in account {}",
                                registry.commodity_name(*commodity),
                                registry.account_name(*account)
                            )
                            .into(),
                            bookings: Booking::create(
                                credit,
                                *account,
                                Decimal::ZERO,
                                *commodity,
                                Some(gain),
                            ),
                            targets: Some(vec![*commodity]),
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
                        account: b.account,
                        other: b.other,
                        commodity: b.commodity,
                        valuation: self.valuation,
                        quantity: b.quantity,
                        value: b.value,
                    })
                })
        });
    }
}

#[derive(Debug, Clone)]
pub struct Row {
    pub date: NaiveDate,
    pub account: AccountID,
    pub other: AccountID,
    pub commodity: CommodityID,
    pub valuation: Option<CommodityID>,
    pub description: Rc<String>,
    pub quantity: Decimal,
    pub value: Option<Decimal>,
}

pub struct Closer {
    dates: Vec<NaiveDate>,
    current: usize,
    quantities: Positions<(AccountID, CommodityID), Decimal>,
    values: Positions<(AccountID, CommodityID), Decimal>,

    equity: AccountID,
}

impl Closer {
    pub fn new(dates: Vec<NaiveDate>, equity: AccountID) -> Self {
        Closer {
            dates,
            equity,
            quantities: Default::default(),
            values: Default::default(),
            current: 0,
        }
    }

    pub fn process(&mut self, r: Row) -> Vec<Row> {
        let mut res = Vec::new();
        if self.current < self.dates.len() {
            if r.date >= self.dates[self.current] {
                let closing_date = self.dates[self.current];
                res.extend(
                    self.quantities
                        .iter()
                        .map(|(k @ (account, commodity), quantity)| Row {
                            date: closing_date,
                            description: Rc::new("".into()),
                            account: *account,
                            other: self.equity,
                            commodity: *commodity,
                            quantity: *quantity,
                            value: self.values.get(k).copied(),
                            valuation: r.valuation,
                        }),
                );
                res.extend(
                    self.quantities
                        .iter()
                        .map(|(k @ (account, commodity), quantity)| Row {
                            date: closing_date,
                            description: Rc::new("".into()),
                            account: self.equity,
                            other: *account,
                            commodity: *commodity,
                            quantity: *quantity,
                            value: self.values.get(k).copied(),
                            valuation: r.valuation,
                        }),
                );

                self.current += 1;
                self.quantities.clear();
            }
            if r.account.account_type.is_ie() {
                self.quantities.add(&(r.account, r.commodity), &r.quantity);
                if let Some(ref value) = r.value {
                    self.values.add(&(r.account, r.commodity), value);
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
