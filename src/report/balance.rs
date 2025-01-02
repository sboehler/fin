use std::{
    fmt::Alignment,
    iter::{self, Sum},
    ops::{AddAssign, Deref},
    rc::Rc,
};

use chrono::NaiveDate;
use regex::Regex;
use rust_decimal::Decimal;

use crate::model::{
    entities::{AccountID, AccountType, CommodityID, Positions},
    journal::Entry,
    registry::Registry,
};

use super::{
    segment_tree::Node,
    table::{Cell, Row, Table},
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
    pub fn align(&self, row: Entry) -> Option<Entry> {
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

    pub fn shorten(&self, account: AccountID) -> Option<AccountID> {
        let name = self.registry.account_name(account);
        for (pattern, n) in &self.patterns {
            if pattern.is_match(&name) {
                return self.registry.shorten(account, *n);
            }
        }
        Some(account)
    }
}

#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug, Ord, PartialOrd)]
pub enum AmountType {
    Value,
    Quantity,
}

#[derive(Default)]
pub struct DatedPositions {
    positions: Positions<AccountID, Position>,
}

impl DatedPositions {
    pub fn add(&mut self, row: Entry) {
        let pos = self.positions.entry(row.account).or_default();
        pos.quantities
            .entry(row.commodity)
            .or_default()
            .add(&row.date, &row.quantity);
        if let Some(value) = row.value {
            pos.values
                .entry(row.commodity)
                .or_default()
                .add(&row.date, &value);
        }
    }

    pub fn map_account<F>(&self, f: F) -> Self
    where
        F: Fn(AccountID) -> Option<AccountID>,
    {
        Self {
            positions: self.positions.map_keys(f),
        }
    }
}

impl Deref for DatedPositions {
    type Target = Positions<AccountID, Position>;

    fn deref(&self) -> &Self::Target {
        &self.positions
    }
}

impl Sum<Entry> for DatedPositions {
    fn sum<I: Iterator<Item = Entry>>(iter: I) -> Self {
        let mut res = Self::default();
        iter.into_iter().for_each(|row| res.add(row));
        res
    }
}

impl FromIterator<Entry> for DatedPositions {
    fn from_iter<T: IntoIterator<Item = Entry>>(iter: T) -> Self {
        let mut res = Self::default();
        iter.into_iter().for_each(|row| res.add(row));
        res
    }
}

impl<'a> FromIterator<(AccountID, &'a Position)> for DatedPositions {
    fn from_iter<T: IntoIterator<Item = (AccountID, &'a Position)>>(iter: T) -> Self {
        let mut res = Self::default();
        iter.into_iter().for_each(|(account, position)| {
            let pos = res.positions.entry(account).or_default();
            pos.quantities += &position.quantities;
            pos.values += &position.values;
        });
        res
    }
}

pub struct MultiperiodTree {
    dates: Vec<NaiveDate>,
    registry: Rc<Registry>,

    root: Node<Position>,
}

#[derive(Default)]
pub struct Position {
    quantities: Positions<CommodityID, Positions<NaiveDate, Decimal>>,
    values: Positions<CommodityID, Positions<NaiveDate, Decimal>>,
}

impl AddAssign<&Position> for Position {
    fn add_assign(&mut self, rhs: &Position) {
        self.quantities += &rhs.quantities;
        self.values += &rhs.values;
    }
}

use AccountType::*;

impl MultiperiodTree {
    pub fn create(
        dates: Vec<NaiveDate>,
        registry: Rc<Registry>,
        dated_positions: &DatedPositions,
    ) -> Self {
        let mut res = Self {
            dates: dates.clone(),
            registry: registry.clone(),
            root: Node::<Position>::default(),
        };
        dated_positions.iter().for_each(|(account, position)| {
            let node = res.lookup(account);
            node.quantities += &position.quantities;
            node.values += &position.values;
        });
        res
    }

    fn lookup<'a>(&'a mut self, account_id: &AccountID) -> &'a mut Node<Position> {
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
        table.add_row(Row::Separator);
        let header = iter::once(Cell::Text {
            text: "Account".to_string(),
            align: Alignment::Center,
            indent: 0,
        })
        .chain(self.dates.iter().map(|d| Cell::Text {
            text: format!("{}", d.format("%Y-%m-%d")),
            align: Alignment::Center,
            indent: 0,
        }))
        .collect();
        table.add_row(Row::Row(header));
        table.add_row(Row::Separator);

        let mut total_al = Position::default();
        [Assets, Liabilities].iter().for_each(|&account_type| {
            if let Some(node) = self.root.children.get(account_type.to_string().as_str()) {
                node.iter_post().for_each(|(_, node)| total_al += node);
                self.render_subtree(&mut table, node, account_type);
                table.add_row(Row::Empty);
            }
        });
        self.render_summary(&mut table, "Total (A+L)", &total_al, false);

        table.add_row(Row::Separator);

        let mut total_eie = Position::default();
        [Equity, Income, Expenses].iter().for_each(|&account_type| {
            if let Some(node) = self.root.children.get(account_type.to_string().as_str()) {
                node.iter_post().for_each(|(_, node)| total_eie += node);
                self.render_subtree(&mut table, node, account_type);
                table.add_row(Row::Empty);
            }
        });
        self.render_summary(&mut table, "Total (E+I+E)", &total_eie, true);

        table.add_row(Row::Separator);

        let mut delta = total_al;
        delta += &total_eie;
        self.render_summary(&mut table, "Delta", &delta, false);
        table.add_row(Row::Separator);
        table
    }

    fn render_summary(&self, table: &mut Table, header: &str, node: &Position, neg: bool) {
        let header_cell = Cell::Text {
            text: header.to_string(),
            indent: 0,
            align: Alignment::Left,
        };
        let total_value = node.values.values().sum::<Positions<NaiveDate, Decimal>>();
        let row = Row::Row(
            iter::once(header_cell)
                .chain(self.dates.iter().map(|date| {
                    total_value
                        .get(date)
                        .map(|v| if neg { -*v } else { *v })
                        .map(|value| Cell::Decimal { value })
                        .unwrap_or(Cell::Empty)
                }))
                .collect(),
        );
        table.add_row(row);
    }

    fn render_subtree(&self, table: &mut Table, root: &Node<Position>, account_type: AccountType) {
        root.iter_pre().for_each(|(v, node)| {
            let text = v
                .last()
                .map(|s| s.to_string())
                .unwrap_or(format!("{}", account_type));
            let header_cell = Cell::Text {
                text,
                indent: 2 * v.len(),
                align: Alignment::Left,
            };
            let total_value = node.values.values().sum::<Positions<NaiveDate, Decimal>>();
            let row = Row::Row(
                iter::once(header_cell)
                    .chain(self.dates.iter().map(|date| {
                        total_value
                            .get(date)
                            .map(|v| match account_type {
                                Assets | Liabilities => *v,
                                Equity | Income | Expenses => -*v,
                            })
                            .map(|value| Cell::Decimal { value })
                            .unwrap_or(Cell::Empty)
                    }))
                    .collect(),
            );
            table.add_row(row);
        });
    }
}
