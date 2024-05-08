use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
    result,
};

use rust_decimal::Decimal;

use crate::model::{journal::Journal, prices::Prices, Commodity};

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
            day.normalized_prices = prices.normalize(t);
        }
    }
}
