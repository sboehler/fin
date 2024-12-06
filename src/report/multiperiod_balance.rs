use std::{
    collections::HashMap,
    iter::{self},
    rc::Rc,
};

use chrono::NaiveDate;

use crate::model::{
    entities::{Amount, Commodity},
    journal::Row,
};

use super::{
    segment_tree::Node,
    table::{self, Table},
};

pub struct MultiperiodBalance {
    dates: Vec<NaiveDate>,

    root: Node<AmountsByCommodity>,
}

#[derive(Default)]
pub struct AmountsByCommodity {
    commodities: HashMap<Rc<Commodity>, Vec<Amount>>,
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
            .or_insert_with(|| vec![Amount::ZERO; self.dates.len()]);
        c[i] += r.amount;
    }

    pub fn print(&self) {
        self.root.iter_pre().for_each(|(v, _)| println!("{:?}", v));
    }

    pub fn render(&self) -> Table {
        let mut t = Table::new(
            &iter::once(0)
                .chain(iter::repeat(1).take(self.dates.len()))
                .collect::<Vec<_>>(),
        );
        t.add_row(table::Row::Separator);
        let header = iter::once(table::Cell::Empty)
            .chain(self.dates.iter().map(|d| table::Cell::Text {
                text: format!("{}", d.format("%Y-%m-%d")),
                align: std::fmt::Alignment::Center,
                indent: 0,
            }))
            .collect();
        t.add_row(table::Row::Row { cells: header });
        t.add_row(table::Row::Separator);
        t
    }
}
