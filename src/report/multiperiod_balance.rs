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

pub struct MultiperiodPositions {
    dates: Vec<NaiveDate>,
    registry: Rc<Registry>,

    positions: Positions<(AccountID, CommodityID), Vector<Amount>>,
}

impl MultiperiodPositions {
    pub fn new(registry: Rc<Registry>, dates: Vec<NaiveDate>) -> Self {
        MultiperiodPositions {
            dates,
            registry,
            positions: Default::default(),
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
            .positions
            .entry((r.account, r.commodity))
            .or_insert_with(|| Vector::new(self.dates.len()));
        v[i] += r.amount;
    }

    pub fn build_tree(&mut self) -> MultiperiodTree {
        let mut root = Node::<Positions<String, Vector<Amount>>>::default();
        self.positions
            .positions()
            .for_each(|((account_id, commodity_id), amount)| {
                let node = self.lookup(&mut root, account_id);
                let commodity_name = self.registry.commodity_name(*commodity_id);
                node.value.add(&commodity_name, amount);
            });
        MultiperiodTree {
            dates: self.dates.clone(),
            root,
        }
    }

    pub fn remap<F>(&self, f: F) -> Self
    where
        F: Fn((AccountID, CommodityID)) -> (AccountID, CommodityID),
    {
        let mut res = Self::new(self.registry.clone(), self.dates.clone());
        res.positions = self.positions.map_keys(f);
        res
    }

    fn lookup<'a>(
        &self,
        root: &'a mut Node<Positions<String, Vector<Amount>>>,
        account_id: &AccountID,
    ) -> &'a mut Node<Positions<String, Vector<Amount>>> {
        let account_name = self.registry.account_name(*account_id);
        let segments = account_name.split(":").collect::<Vec<_>>();
        root.lookup_or_create_mut_node(&segments)
    }
}

pub trait Converter {
    fn convert(&self, key: (AccountID, CommodityID)) -> (Option<AccountID>, Option<CommodityID>);
}

pub struct MultiperiodTree {
    dates: Vec<NaiveDate>,

    root: Node<Positions<String, Vector<Amount>>>,
}

impl MultiperiodTree {
    pub fn render(&self) -> Table {
        let mut table = Table::new(
            iter::once(0)
                .chain(iter::repeat(1).take(self.dates.len()))
                .collect::<Vec<_>>(),
        );
        table.add_row(table::Row::Separator);
        let header = iter::once(table::Cell::Empty)
            .chain(self.dates.iter().map(|d| table::Cell::Text {
                text: format!("{}", d.format("%Y-%m-%d")),
                align: std::fmt::Alignment::Center,
                indent: 0,
            }))
            .collect();
        table.add_row(table::Row::Row { cells: header });
        table.add_row(table::Row::Separator);

        self.root.iter_pre().for_each(|(v, k)| {
            if v.is_empty() {
                return;
            }
            let header_cell = table::Cell::Text {
                indent: 2 * (v.len() - 1),
                text: v.last().unwrap().to_string(),
                align: Alignment::Left,
            };

            let value_cells = if k.len() == 0 {
                self.dates
                    .iter()
                    .map(|_| table::Cell::Empty)
                    .collect::<Vec<_>>()
            } else {
                k.positions()
                    .map(|(_, v)| v)
                    .sum::<Vector<Amount>>()
                    .into_elements()
                    .map(|a| table::Cell::Decimal { value: a.value })
                    .collect::<Vec<_>>()
            };
            table.add_row(table::Row::Row {
                cells: iter::once(header_cell).chain(value_cells).collect(),
            });
        });
        table.add_row(table::Row::Separator);
        table
    }
}
