use std::{collections::HashMap, rc::Rc, result};

use chrono::NaiveDate;
use rust_decimal::Decimal;

use super::{
    entities::{CommodityID, Price},
    error::ModelError,
    registry::Registry,
};

#[derive(Default)]
pub struct Prices {
    date: NaiveDate,
    prices: HashMap<CommodityID, HashMap<CommodityID, Decimal>>,
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
            .entry(price.target)
            .or_default()
            .insert(price.commodity, price.price);
        self.prices
            .entry(price.commodity)
            .or_default()
            .insert(price.target, Decimal::ONE / price.price);
    }

    pub fn normalize(&self, target: CommodityID) -> NormalizedPrices {
        let mut prices = HashMap::default();
        self.normalize_rec(target, Decimal::ONE, &mut prices);
        NormalizedPrices {
            date: self.date,
            target: target,
            prices,
        }
    }

    fn normalize_rec(
        &self,
        target: CommodityID,
        target_price: Decimal,
        prices: &mut HashMap<CommodityID, Decimal>,
    ) {
        prices.insert(target, target_price);
        if let Some(target_denominated) = self.prices.get(&target) {
            for (neighbor, price) in target_denominated {
                if prices.contains_key(neighbor) {
                    continue;
                }
                self.normalize_rec(*neighbor, price * target_price, prices)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct NormalizedPrices {
    date: NaiveDate,
    target: CommodityID,
    prices: HashMap<CommodityID, Decimal>,
}

type Result<T> = result::Result<T, ModelError>;

impl NormalizedPrices {
    pub fn new(commodity: CommodityID) -> Self {
        NormalizedPrices {
            date: NaiveDate::default(),
            target: commodity,
            prices: HashMap::default(),
        }
    }
    pub fn valuate(
        &self,
        registry: &Rc<Registry>,
        quantity: &Decimal,
        commodity: CommodityID,
    ) -> Result<Decimal> {
        if let Some(price) = self.prices.get(&commodity) {
            return Ok((quantity * price)
                .round_dp_with_strategy(8, rust_decimal::RoundingStrategy::MidpointAwayFromZero));
        }
        Err(ModelError::NoPriceFound {
            date: self.date,
            commodity_name: registry.commodity_name(commodity),
            target_name: registry.commodity_name(self.target),
        })
    }
}
