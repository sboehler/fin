use std::{collections::HashMap, rc::Rc, result};

use chrono::NaiveDate;
use rust_decimal::Decimal;

use super::{
    entities::{Commodity, Price},
    error::ModelError,
};

#[derive(Default)]
pub struct Prices {
    date: NaiveDate,
    prices: HashMap<Rc<Commodity>, HashMap<Rc<Commodity>, Decimal>>,
}

impl Prices {
    pub fn insert(&mut self, price: &Price) {
        assert!(
            price.date >= self.date,
            "can't add price with date {} to prices with running date {}",
            price.date,
            self.date
        );
        self.date = price.date;
        self.prices
            .entry(price.target.clone())
            .or_default()
            .insert(price.commodity.clone(), price.price);
        self.prices
            .entry(price.commodity.clone())
            .or_default()
            .insert(price.target.clone(), Decimal::ONE / price.price);
    }

    pub fn normalize(&self, target: &Rc<Commodity>) -> NormalizedPrices {
        let mut prices = HashMap::default();
        self.normalize_rec(target, Decimal::ONE, &mut prices);
        NormalizedPrices {
            date: self.date,
            target: target.clone(),
            prices,
        }
    }

    fn normalize_rec(
        &self,
        target: &Rc<Commodity>,
        target_price: Decimal,
        prices: &mut HashMap<Rc<Commodity>, Decimal>,
    ) {
        prices.insert(target.clone(), target_price);
        if let Some(target_denominated) = self.prices.get(target) {
            for (neighbor, price) in target_denominated {
                if prices.contains_key(neighbor) {
                    continue;
                }
                self.normalize_rec(neighbor, price * target_price, prices)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct NormalizedPrices {
    date: NaiveDate,
    target: Rc<Commodity>,
    prices: HashMap<Rc<Commodity>, Decimal>,
}

type Result<T> = result::Result<T, ModelError>;

impl NormalizedPrices {
    pub fn new(commodity: &Rc<Commodity>) -> Self {
        NormalizedPrices {
            date: NaiveDate::default(),
            target: commodity.clone(),
            prices: HashMap::default(),
        }
    }
    pub fn valuate(&self, quantity: &Decimal, commodity: &Rc<Commodity>) -> Result<Decimal> {
        if let Some(p) = self.prices.get(commodity) {
            return Ok(quantity * p);
        }
        Err(ModelError::NoPriceFound {
            date: self.date,
            commodity: commodity.clone(),
            target: self.target.clone(),
        })
    }
}
