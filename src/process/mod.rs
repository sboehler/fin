use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
    result,
};

use chrono::NaiveDate;
use rust_decimal::Decimal;
use thiserror::Error;

use crate::model::{
    entities::{Account, Assertion},
    journal::{Day, Journal},
};
use crate::model::{
    entities::{Booking, Commodity, Transaction},
    error::ModelError,
};
use crate::model::{
    entities::{Close, Open},
    prices::Prices,
};

#[derive(Error, Eq, PartialEq, Debug)]
#[error("process error")]
pub enum ProcessError {
    AccountAlreadyOpen {
        open: Open,
    },
    TransactionAccountNotOpen {
        transaction: Transaction,
        account: Rc<Account>,
    },
    AssertionAccountNotOpen {
        assertion: Assertion,
    },
    AssertionIncorrectBalance {
        assertion: Assertion,
        actual: Decimal,
    },
    CloseNonzeroBalance {
        close: Close,
        commodity: Rc<Commodity>,
        balance: Decimal,
    },
    ModelError(#[from] ModelError),
}

type Result<T> = result::Result<T, ProcessError>;

pub fn check(journal: &Journal) -> Result<()> {
    let mut quantities = HashMap::new();
    let mut accounts = HashSet::new();

    for day in journal {
        day.openings.iter().try_for_each(|o| {
            if !accounts.insert(o.account.clone()) {
                return Err(ProcessError::AccountAlreadyOpen { open: o.clone() });
            }
            Ok(())
        })?;
        day.transactions.iter().try_for_each(|t| {
            t.postings.iter().try_for_each(|b| {
                if !accounts.contains(&b.account) {
                    return Err(ProcessError::TransactionAccountNotOpen {
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
                return Err(ProcessError::AssertionAccountNotOpen {
                    assertion: a.clone(),
                });
            }
            let balance = quantities
                .get(&(a.account.clone(), a.commodity.clone()))
                .copied()
                .unwrap_or_default();
            if balance != a.balance {
                return Err(ProcessError::AssertionIncorrectBalance {
                    assertion: a.clone(),
                    actual: balance,
                });
            }
            Ok(())
        })?;
        day.closings.iter().try_for_each(|c| {
            for (pos, qty) in quantities.iter() {
                if pos.0 == c.account && !qty.is_zero() {
                    return Err(ProcessError::CloseNonzeroBalance {
                        close: c.clone(),
                        commodity: pos.1.clone(),
                        balance: *qty,
                    });
                }
            }
            accounts.remove(&c.account);
            quantities.retain(|(a, _), _| a != &c.account);
            Ok(())
        })?;
        day.quantities
            .set(quantities.clone())
            .expect("quantities have been set already");
    }
    Ok(())
}

pub fn compute_valuation(journal: &Journal, valuation: Option<Rc<Commodity>>) -> Result<()> {
    let mut prev_day: &Day = &Day::new(NaiveDate::default());
    let mut prices = Prices::default();

    if let Some(target) = valuation {
        for day in journal.days.values() {
            day.prices.iter().for_each(|p| prices.insert(p));
            let cur_prices = prices.normalize(&target);

            // valuate transactions
            day.transactions
                .iter()
                .flat_map(|t| t.postings.iter())
                .try_for_each(|booking| {
                    cur_prices
                        .valuate(&booking.quantity, &booking.commodity)
                        .map(|v| booking.value.set(v))
                        .map_err(ProcessError::ModelError)
                })?;

            let prev_quantities = prev_day
                .quantities
                .get()
                .expect("quantities are not initialized");

            let prev_prices = prev_day
                .normalized_prices
                .get()
                .expect("previous normalized prices are not yet computed");

            // compute valuation gains since last day
            let gains = prev_quantities
                .iter()
                .map(|((account, commodity), qty)| {
                    if qty.is_zero() {
                        return Ok(None);
                    }
                    let v_prev = prev_prices.valuate(qty, commodity)?;
                    let v_cur = cur_prices.valuate(qty, commodity)?;
                    let gain = v_cur - v_prev;
                    if gain.is_zero() {
                        return Ok(None);
                    }
                    let credit = journal.registry.borrow_mut().account("Income:Valuation")?;
                    Ok(Some(Transaction {
                        date: day.date,
                        rng: None,
                        description: format!(
                            "Adjust value of {} in account {}",
                            commodity.name, account.name
                        ),
                        postings: Booking::create(
                            credit,
                            account.clone(),
                            Decimal::ZERO,
                            commodity.clone(),
                            gain,
                        ),
                        targets: Some(vec![commodity.clone()]),
                    }))
                })
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();

            day.gains.set(gains).expect("gains have already been set");
            day.normalized_prices
                .set(cur_prices)
                .expect("normalized prices have already been set");

            prev_day = day
        }
    }
    Ok(())
}
