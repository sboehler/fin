use std::{collections::HashMap, rc::Rc};

use rust_decimal::Decimal;

use super::{Commodity, Price};

pub struct Prices {
    prices: HashMap<Rc<Commodity>, HashMap<Rc<Commodity>, Decimal>>,
}

pub type NormalizedPrices = HashMap<Rc<Commodity>, Decimal>;

impl Default for Prices {
    fn default() -> Self {
        Self {
            prices: Default::default(),
        }
    }
}
impl Prices {
    pub fn insert(&mut self, price: &Price) {
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
        let mut res = NormalizedPrices::default();
        self.normalize_rec(&target, Decimal::ONE, &mut res);
        res
    }

    fn normalize_rec(
        &self,
        target: &Rc<Commodity>,
        target_price: Decimal,
        res: &mut NormalizedPrices,
    ) {
        res.insert(target.clone(), target_price);
        if let Some(target_denominated) = self.prices.get(target) {
            for (neighbor, price) in target_denominated {
                self.normalize_rec(neighbor, price * target_price, res)
            }
        }
    }
}
