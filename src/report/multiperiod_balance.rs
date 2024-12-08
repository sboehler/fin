use std::{
    fmt::Alignment,
    iter::{self},
    rc::Rc,
};

use chrono::NaiveDate;

use crate::model::{
    entities::{AccountID, Amount, CommodityID, Positions, Vector},
    journal::Row,
    registry::Registry,
};

use super::{
    segment_tree::Node,
    table::{self, Table},
};

pub struct MultiperiodBalance {
    dates: Vec<NaiveDate>,
    registry: Rc<Registry>,

    balances: Positions<(AccountID, CommodityID), Vector<Amount>>,

    root: Node<AmountsByCommodity>,
}

impl MultiperiodBalance {
    pub fn new(registry: Rc<Registry>, dates: Vec<NaiveDate>) -> Self {
        MultiperiodBalance {
            dates,
            registry,
            balances: Default::default(),
            root: Default::default(),
        }
    }

    fn align(&self, date: NaiveDate) -> Option<usize> {
        match self.dates.binary_search(&date) {
            Err(i) if i >= self.dates.len() => None,
            Ok(i) | Err(i) => Some(i),
        }
    }

    pub fn register(&mut self, r: Row) {
        let Some(i) = self.align(r.date) else { return };
        let v = self
            .balances
            .get_or_create((r.account, r.commodity), || Vector::new(self.dates.len()));
        v[i] += r.amount;
    }

    pub fn print(&self) {
        self.balances
            .positions()
            .for_each(|((account_id, commodity_id), amounts)| {
                let account_name = self.registry.account_name(*account_id);
                let commodity_name = self.registry.commodity_name(*commodity_id);
                println!("{account_name} {commodity_name} {amounts:?} ")
            });
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

        self.root.iter_pre().for_each(|(v, k)| {
            let header_cell = table::Cell::Text {
                indent: v.len() - 1,
                text: v.join(":"),
                align: Alignment::Left,
            };
            let value_cells = k
                .amounts_by_commodity
                .positions()
                .map(|(_, v)| v)
                .sum::<Vector<Amount>>()
                .into_elements()
                .map(|a| table::Cell::Decimal { value: a.value });
            t.add_row(table::Row::Row {
                cells: iter::once(header_cell).chain(value_cells).collect(),
            });
        });
        t
    }
}

#[derive(Default)]
pub struct AmountsByCommodity {
    amounts_by_commodity: Positions<CommodityID, Vector<Amount>>,
}

impl AmountsByCommodity {
    pub fn sum(&self) -> Vector<Amount> {
        self.amounts_by_commodity.positions().map(|(_, v)| v).sum()
    }
}
