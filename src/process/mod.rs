use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
    result,
};

use rust_decimal::Decimal;

use crate::model::{
    entities::Commodity,
    prices::{self},
};
use crate::model::{entities::Transaction, prices::Prices};
use crate::model::{
    entities::{Account, Booking},
    journal::Journal,
};

pub fn check(journal: &mut Journal) -> result::Result<(), String> {
    let mut quantities = HashMap::new();
    let mut accounts = HashSet::new();

    for day in journal {
        day.openings.iter().try_for_each(|o| {
            if !accounts.insert(o.account.clone()) {
                return Err("already opened");
            }
            Ok(())
        })?;
        day.transactions
            .iter()
            .flat_map(|t| t.postings.iter())
            .try_for_each(|b| {
                if !accounts.contains(&b.account) {
                    return Err("not open");
                }
                quantities
                    .entry((b.account.clone(), b.commodity.clone()))
                    .and_modify(|q| *q += b.quantity)
                    .or_insert(b.quantity);
                Ok(())
            })?;
        day.assertions.iter().try_for_each(|a| {
            if !accounts.contains(&a.account) {
                return Err("not open".into());
            }
            let balance = quantities
                .get(&(a.account.clone(), a.commodity.clone()))
                .copied()
                .unwrap_or(Decimal::ZERO);
            if balance != a.balance {
                return Err(format!(
                    "mismatch {:?} {} {}",
                    a.account, balance, a.balance
                ));
            }
            Ok(())
        })?;
        day.closings.iter().try_for_each(|c| {
            quantities
                .iter()
                .filter(|((a, _), _)| *a == c.account)
                .try_for_each(|(_, q)| {
                    if !q.is_zero() {
                        Err(format!("{:?}: not zero", c))
                    } else {
                        Ok(())
                    }
                })?;
            accounts.remove(&c.account);
            Ok::<_, String>(())
        })?;
    }
    Ok(())
}

pub fn compute_prices(journal: &mut Journal, target: &Option<Rc<Commodity>>) {
    if let Some(t) = target {
        let mut prices = Prices::default();
        for day in journal {
            day.prices.iter().for_each(|p| prices.insert(p));
            day.normalized_prices.set(prices.normalize(t)).unwrap();
        }
    }
}

pub fn valuate(journal: &Journal, target: Option<Rc<Commodity>>) -> Result<(), String> {
    let mut quantities: HashMap<(Rc<Account>, Rc<Commodity>), Decimal> = HashMap::new();

    let mut prev_prices: &prices::NormalizedPrices = &Default::default();
    if let Some(t) = target {
        for (_, day) in journal.days.iter() {
            let mut gains = Vec::new();
            for (pos, qty) in quantities.iter() {
                if pos.1 == t {
                    continue;
                }
                if qty.is_zero() {
                    continue;
                }
                let cp = day.normalized_prices.get().unwrap();
                let prev = prev_prices.get(&pos.1).unwrap();
                let current = cp.get(&pos.1).unwrap();
                let delta = current - prev;
                if delta.is_zero() {
                    continue;
                }
                let gain = delta * qty;
                let credit = journal
                    .registry
                    .borrow_mut()
                    .account("Income:Valuation")
                    .unwrap();
                gains.push(Transaction {
                    date: day.date,
                    rng: None,
                    description: format!(
                        "Adjust value of {} in account {}",
                        pos.1.name, pos.0.name
                    ),
                    postings: Booking::create(
                        credit,
                        pos.0.clone(),
                        Decimal::ZERO,
                        pos.1.clone(),
                        gain,
                    ),
                    targets: Some(vec![pos.1.clone()]),
                })
            }
            day.gains.set(gains).unwrap();

            for trx in day.transactions.iter() {
                for b in trx.postings.iter() {
                    // update quantities
                    if !b.quantity.is_zero() && b.account.account_type.is_al() {
                        quantities
                            .entry((b.account.clone(), b.commodity.clone()))
                            .and_modify(|q| *q += b.quantity)
                            .or_insert(b.quantity);
                    }
                    // valuate transaction
                    b.value.set(if t == b.commodity {
                        b.quantity
                    } else {
                        let p = day
                            .normalized_prices
                            .get()
                            .unwrap()
                            .get(&b.commodity)
                            .to_owned()
                            .unwrap();
                        p * b.quantity
                    })
                }
            }
            prev_prices = day.normalized_prices.get().unwrap()
        }
    }
    Ok(())
}
