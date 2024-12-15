use std::{
    fmt::Alignment,
    iter::{self},
    rc::Rc,
};

use chrono::NaiveDate;
use rust_decimal::Decimal;

use crate::model::{
    entities::{AccountID, CommodityID, Positions},
    journal::Row,
    registry::Registry,
};

use super::{
    segment_tree::Node,
    table::{self, Table},
};

#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug, Ord, PartialOrd)]
pub enum AmountType {
    Value,
    Quantity,
}

pub struct MultiperiodPositions {
    dates: Vec<NaiveDate>,
    registry: Rc<Registry>,

    positions: Positions<(AccountID, CommodityID, AmountType), Positions<NaiveDate, Decimal>>,
}

impl MultiperiodPositions {
    pub fn new(registry: Rc<Registry>, dates: Vec<NaiveDate>) -> Self {
        MultiperiodPositions {
            dates,
            registry,
            positions: Default::default(),
        }
    }

    fn align(&self, date: NaiveDate) -> Option<NaiveDate> {
        match self.dates.binary_search(&date) {
            Err(i) if i >= self.dates.len() => None,
            Ok(i) | Err(i) => Some(self.dates[i]),
        }
    }

    pub fn register(&mut self, r: Row) {
        let Some(date) = self.align(r.date) else {
            return;
        };
        self.positions
            .entry((r.account, r.commodity, AmountType::Quantity))
            .or_default()
            .entry(date)
            .and_modify(|v| *v += r.quantity)
            .or_insert(r.quantity);
        if let Some(value) = r.value {
            self.positions
                .entry((r.account, r.commodity, AmountType::Value))
                .or_default()
                .entry(date)
                .and_modify(|v| *v += value)
                .or_insert(value);
        };
    }

    pub fn remap<F>(&self, f: F) -> Self
    where
        F: Fn((AccountID, CommodityID, AmountType)) -> (AccountID, CommodityID, AmountType),
    {
        let mut res = Self::new(self.registry.clone(), self.dates.clone());
        res.positions = self.positions.map_keys(f);
        res
    }
}

pub struct MultiperiodTree {
    dates: Vec<NaiveDate>,
    registry: Rc<Registry>,

    root: Node<TreeNode>,
}

#[derive(Default)]
pub struct TreeNode {
    quantities: Positions<CommodityID, Positions<NaiveDate, Decimal>>,
    values: Positions<CommodityID, Positions<NaiveDate, Decimal>>,
}

impl MultiperiodTree {
    pub fn new(multiperiod_positions: MultiperiodPositions) -> MultiperiodTree {
        let registry = multiperiod_positions.registry;
        let mut res = Self {
            dates: multiperiod_positions.dates.clone(),
            registry: registry.clone(),
            root: Node::<TreeNode>::default(),
        };
        multiperiod_positions.positions.positions().for_each(
            |((account_id, commodity_id, amount_type), amount)| {
                let node = res.lookup(account_id);
                match amount_type {
                    AmountType::Value => node.value.values.add(commodity_id, amount),

                    AmountType::Quantity => node.value.quantities.add(commodity_id, amount),
                }
            },
        );
        res
    }

    fn lookup<'a>(&'a mut self, account_id: &AccountID) -> &'a mut Node<TreeNode> {
        let account_name = self.registry.account_name(*account_id);
        let segments = account_name.split(":").collect::<Vec<_>>();
        self.root.lookup_or_create_mut_node(&segments)
    }

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

        self.root.iter_pre().for_each(|(v, node)| {
            if v.is_empty() {
                return;
            }
            let header_cell = table::Cell::Text {
                indent: 2 * (v.len() - 1),
                text: v.last().unwrap().to_string(),
                align: Alignment::Left,
            };

            let value_cells = if node.values.is_empty() {
                self.dates
                    .iter()
                    .map(|_| table::Cell::Empty)
                    .collect::<Vec<_>>()
            } else {
                let total = node
                    .values
                    .positions()
                    .map(|(_, v)| v)
                    .sum::<Positions<NaiveDate, Decimal>>();
                self.dates
                    .iter()
                    .map(|date| {
                        total
                            .get(date)
                            .map(|value| table::Cell::Decimal { value: *value })
                            .unwrap_or(table::Cell::Empty)
                    })
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
