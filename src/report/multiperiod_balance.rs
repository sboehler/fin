use std::{collections::HashMap, rc::Rc};

use chrono::NaiveDate;
use rust_decimal::Decimal;

use crate::model::{entities::Commodity, journal::Row};

use super::segment_tree::Node;

pub struct MultiperiodBalance {
    dates: Vec<NaiveDate>,

    root: Node<AmountsByCommodity>,
}

impl MultiperiodBalance {
    pub fn new(dates: Vec<NaiveDate>) -> Self {
        MultiperiodBalance {
            dates,
            root: Default::default(),
        }
    }

    fn align(&self, date: NaiveDate) -> Option<usize> {
        match self.dates.binary_search(&date) {
            Ok(i) => Some(i),
            Err(i) if i >= self.dates.len() => None,
            Err(i) => Some(i),
        }
    }

    pub fn register(&mut self, r: Row) {
        let Some(i) = self.align(r.date) else { return };
        let c = self
            .root
            .lookup_or_create_mut(&r.account.name.split(":").collect::<Vec<&str>>())
            .commodities
            .entry(r.commodity.clone())
            .or_insert_with(|| vec![(Decimal::ZERO, Decimal::ZERO); self.dates.len()]);
        c[i].0 += r.quantity;
        c[i].1 += r.value;
    }
}

#[derive(Default)]
pub struct AmountsByCommodity {
    pub commodities: HashMap<Rc<Commodity>, Vec<(Decimal, Decimal)>>,
}
