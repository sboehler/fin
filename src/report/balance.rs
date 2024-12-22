use std::{
    fmt::Alignment,
    iter::{self, Sum},
    ops::Deref,
    rc::Rc,
};

use chrono::NaiveDate;
use regex::Regex;
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

pub struct Aligner {
    dates: Vec<NaiveDate>,
}

impl Aligner {
    pub fn new(dates: Vec<NaiveDate>) -> Self {
        Self { dates }
    }
}

impl Aligner {
    pub fn align(&self, row: Row) -> Option<Row> {
        match self.dates.binary_search(&row.date) {
            Err(i) if i >= self.dates.len() => None,
            Ok(i) | Err(i) => {
                let mut res = row.clone();
                res.date = self.dates[i];
                Some(res)
            }
        }
    }
}

pub struct Shortener {
    registry: Rc<Registry>,
    patterns: Vec<(Regex, usize)>,
}

impl Shortener {
    pub fn new(registry: Rc<Registry>, patterns: Vec<(Regex, usize)>) -> Self {
        Shortener { registry, patterns }
    }

    pub fn shorten(&self, key: Key) -> Option<Key> {
        let name = self.registry.account_name(key.account_id);
        for (pattern, n) in &self.patterns {
            if pattern.is_match(&name) {
                return self
                    .registry
                    .shorten(key.account_id, *n)
                    .map(|mapped_id| Key {
                        account_id: mapped_id,
                        commodity_id: key.commodity_id,
                        value_type: key.value_type,
                    });
            }
        }
        Some(key)
    }
}

#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug, Ord, PartialOrd)]
pub enum AmountType {
    Value,
    Quantity,
}

#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug, Ord, PartialOrd)]
pub struct Key {
    pub account_id: AccountID,
    pub commodity_id: CommodityID,
    pub value_type: AmountType,
}

impl Key {
    fn new(account_id: AccountID, commodity_id: CommodityID, value_type: AmountType) -> Self {
        Self {
            account_id,
            commodity_id,
            value_type,
        }
    }
}
#[derive(Default)]
pub struct DatedPositions {
    positions: Positions<Key, Positions<NaiveDate, Decimal>>,
}

impl DatedPositions {
    fn add(&mut self, row: Row) {
        self.positions
            .entry(Key::new(row.account, row.commodity, AmountType::Quantity))
            .or_default()
            .entry(row.date)
            .and_modify(|v| *v += row.quantity)
            .or_insert(row.quantity);
        if let Some(value) = row.value {
            self.positions
                .entry(Key::new(row.account, row.commodity, AmountType::Value))
                .or_default()
                .entry(row.date)
                .and_modify(|v| *v += value)
                .or_insert(value);
        };
    }
}

impl Deref for DatedPositions {
    type Target = Positions<Key, Positions<NaiveDate, Decimal>>;

    fn deref(&self) -> &Self::Target {
        &self.positions
    }
}

impl Sum<Row> for DatedPositions {
    fn sum<I: Iterator<Item = Row>>(iter: I) -> Self {
        let mut res = Self::default();
        iter.into_iter().for_each(|row| res.add(row));
        res
    }
}

impl FromIterator<Row> for DatedPositions {
    fn from_iter<T: IntoIterator<Item = Row>>(iter: T) -> Self {
        let mut res = Self::default();
        iter.into_iter().for_each(|row| res.add(row));
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
    pub fn new(dates: Vec<NaiveDate>, registry: Rc<Registry>) -> MultiperiodTree {
        Self {
            dates,
            registry,
            root: Node::<TreeNode>::default(),
        }
    }

    pub fn add(&mut self, key: Key, amount: &Positions<NaiveDate, Decimal>) {
        let node = self.lookup(&key.account_id);
        match &key.value_type {
            AmountType::Value => node.value.values.add(&key.commodity_id, amount),
            AmountType::Quantity => node.value.quantities.add(&key.commodity_id, amount),
        }
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
        let header = iter::once(table::Cell::Text {
            text: "Account".to_string(),
            align: Alignment::Center,
            indent: 0,
        })
        .chain(self.dates.iter().map(|d| table::Cell::Text {
            text: format!("{}", d.format("%Y-%m-%d")),
            align: std::fmt::Alignment::Center,
            indent: 0,
        }))
        .collect();
        table.add_row(table::Row::Row { cells: header });
        table.add_row(table::Row::Separator);

        if let Some(assets) = self.root.children.get("Assets") {
            self.render_subtree(&mut table, assets, "Assets");
            table.add_row(table::Row::Empty);
        }
        if let Some(liabilities) = self.root.children.get("Liabilities") {
            self.render_subtree(&mut table, liabilities, "Liabilities");
            table.add_row(table::Row::Empty);
        }
        self.render_empty_row_with_header(&mut table, "Total (A+L)");
        table.add_row(table::Row::Separator);
        if let Some(equity) = self.root.children.get("Equity") {
            self.render_subtree(&mut table, equity, "Equity");
            table.add_row(table::Row::Empty);
        }
        if let Some(income) = self.root.children.get("Income") {
            self.render_subtree(&mut table, income, "Income");
            table.add_row(table::Row::Empty);
        }
        if let Some(expenses) = self.root.children.get("Expenses") {
            self.render_subtree(&mut table, expenses, "Expenses");
            table.add_row(table::Row::Empty);
        }
        self.render_empty_row_with_header(&mut table, "Total (E+I+E)");
        table.add_row(table::Row::Separator);
        self.render_empty_row_with_header(&mut table, "Delta");
        table.add_row(table::Row::Separator);
        table
    }

    fn render_empty_row_with_header(&self, table: &mut Table, header: &str) {
        let header_cell = table::Cell::Text {
            indent: 0,
            text: header.to_string(),
            align: Alignment::Left,
        };
        let value_cells = self
            .dates
            .iter()
            .map(|_| table::Cell::Empty)
            .collect::<Vec<_>>();
        table.add_row(table::Row::Row {
            cells: iter::once(header_cell).chain(value_cells).collect(),
        });
    }

    fn render_subtree(&self, table: &mut Table, root: &Node<TreeNode>, header: &str) {
        root.iter_pre().for_each(|(v, node)| {
            let header_cell = table::Cell::Text {
                indent: 2 * (v.len()),
                text: v.last().unwrap_or(&header).to_string(),
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
                    .iter()
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
    }
}

impl<'a> Extend<(Key, &'a Positions<NaiveDate, Decimal>)> for MultiperiodTree {
    fn extend<T: IntoIterator<Item = (Key, &'a Positions<NaiveDate, Decimal>)>>(
        &mut self,
        iter: T,
    ) {
        iter.into_iter()
            .for_each(|(key, amount)| self.add(key, amount))
    }
}
