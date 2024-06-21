use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
    result,
};

pub mod cpr;

use rust_decimal::Decimal;
use thiserror::Error;

use crate::model::{
    entities::{Account, Assertion},
    journal::{Journal, Positions},
    prices::NormalizedPrices,
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
pub enum ProcessError {
    #[error("error processing {open:?}: account is already open")]
    AccountAlreadyOpen { open: Open },
    #[error("error booking transaction {transaction:?}: account {account:?} is not open")]
    TransactionAccountNotOpen {
        transaction: Transaction,
        account: Rc<Account>,
    },
    #[error("error processing assertion {assertion:?}: account is not open")]
    AssertionAccountNotOpen { assertion: Assertion },
    #[error("assertion {assertion:?} failed: balance is {actual:?}")]
    AssertionIncorrectBalance {
        assertion: Assertion,
        actual: Decimal,
    },
    #[error("error closing {close:?}: commodity {commodity:?} has non-zero balance {balance:?}")]
    CloseNonzeroBalance {
        close: Close,
        commodity: Rc<Commodity>,
        balance: Decimal,
    },
    #[error("{0}")]
    ModelError(#[from] ModelError),
}

// impl Display for ProcessError {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         match self {
//             ProcessError::AccountAlreadyOpen { open } => {
//                 writeln!(
//                     f,
//                     "error processing open directive on {date}: account {account} is already open",
//                     account = open.account,
//                     date = open.date,
//                 );
//                 writeln!(f);

//             }
//             ProcessError::TransactionAccountNotOpen {
//                 transaction,
//                 account,
//             } => todo!(),
//             ProcessError::AssertionAccountNotOpen { assertion } => todo!(),
//             ProcessError::AssertionIncorrectBalance { assertion, actual } => todo!(),
//             ProcessError::CloseNonzeroBalance {
//                 close,
//                 commodity,
//                 balance,
//             } => todo!(),
//             ProcessError::ModelError(_) => todo!(),
//         }
//         todo!()
//     }
// }

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

pub fn compute_prices(journal: &Journal, valuation: Option<Rc<Commodity>>) -> Result<()> {
    let Some(target) = valuation else {
        return Ok(());
    };
    let mut prices = Prices::default();
    for day in journal.days.values() {
        day.prices.iter().for_each(|p| prices.insert(p));
        day.normalized_prices
            .set(prices.normalize(&target))
            .expect("normalized prices have already been set");
    }
    Ok(())
}

pub fn valuate_transactions(journal: &Journal, valuation: Option<Rc<Commodity>>) -> Result<()> {
    if valuation.is_none() {
        return Ok(());
    };
    for day in journal.days.values() {
        let cur_prices = day
            .normalized_prices
            .get()
            .expect("normalized prices have not been set");

        day.transactions
            .iter()
            .flat_map(|t| t.postings.iter())
            .try_for_each(|booking| {
                cur_prices
                    .valuate(&booking.quantity, &booking.commodity)
                    .map(|v| booking.value.set(v))
                    .map_err(ProcessError::ModelError)
            })?;
    }
    Ok(())
}

pub fn compute_gains(journal: &Journal, valuation: Option<Rc<Commodity>>) -> Result<()> {
    let Some(target) = valuation else {
        return Ok(());
    };
    let empty_q = Positions::new();
    let empty_p: NormalizedPrices = NormalizedPrices::new(target);

    let mut prev_q = &empty_q;
    let mut prev_p = &empty_p;

    for day in journal.days.values() {
        let cur_prices = day
            .normalized_prices
            .get()
            .expect("normalized prices have not been set");

        // compute valuation gains since last day
        let gains = prev_q
            .iter()
            .map(|((account, commodity), qty)| {
                if qty.is_zero() {
                    return Ok(None);
                }
                let v_prev = prev_p.valuate(qty, commodity)?;
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
        prev_q = day
            .quantities
            .get()
            .expect("quantities are not initialized");
        prev_p = cur_prices;
    }
    Ok(())
}
