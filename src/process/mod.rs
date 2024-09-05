use std::{rc::Rc, result};

pub mod cpr;

use rust_decimal::Decimal;

use crate::model::prices::Prices;
use crate::model::{
    entities::{Booking, Commodity, Transaction},
    error::ModelError,
};
use crate::model::{
    journal::{Journal, Positions},
    prices::NormalizedPrices,
};

type Result<T> = result::Result<T, ModelError>;

pub fn compute_prices(journal: &Journal, valuation: Option<&Rc<Commodity>>) -> Result<()> {
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

pub fn valuate_transactions(journal: &Journal, valuation: Option<&Rc<Commodity>>) -> Result<()> {
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
            .flat_map(|t| t.bookings.iter())
            .try_for_each(|booking| {
                cur_prices
                    .valuate(&booking.quantity, &booking.commodity)
                    .map(|v| booking.value.set(v))
            })?;
    }
    Ok(())
}

pub fn compute_gains(journal: &Journal, valuation: Option<&Rc<Commodity>>) -> Result<()> {
    let Some(target) = valuation else {
        return Ok(());
    };
    let mut q = Positions::new();
    let empty_p: NormalizedPrices = NormalizedPrices::new(target);

    let mut prev_p = &empty_p;

    let credit = journal.registry.account("Income:Valuation")?;

    for day in journal.days.values() {
        let cur_prices = day
            .normalized_prices
            .get()
            .expect("normalized prices have not been set");

        // compute valuation gains since last day
        let gains = q
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
                Ok(Some(Transaction {
                    date: day.date,
                    rng: None,
                    description: format!(
                        "Adjust value of {} in account {}",
                        commodity.name, account.name
                    ),
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
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

        day.gains.set(gains).expect("gains have already been set");

        day.transactions
            .iter()
            .flat_map(|t| t.bookings.iter())
            .filter(|b| b.account.account_type.is_al())
            .for_each(|b| {
                q.entry((b.account.clone(), b.commodity.clone()))
                    .and_modify(|q| *q += b.quantity)
                    .or_insert(b.quantity);
            });
        prev_p = cur_prices;
    }
    Ok(())
}
