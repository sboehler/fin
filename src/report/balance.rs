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

    cumulative: bool,

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
        cumulative: bool,
        dated_positions: &DatedPositions,
    ) -> Self {
        let mut res = Self {
            dates: dates.clone(),
            registry: registry.clone(),
            cumulative,
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
        self.render_header(&mut table);
        table.add_row(Row::Separator);

        let mut total_al = Position::default();
        for account_type in [Assets, Liabilities] {
            let header = account_type.to_string();
            let Some(node) = self.root.children.get(&header) else {
                continue;
            };
            node.iter_post().for_each(|(_, node)| total_al += node);
            self.render_subtree(&mut table, node, header, false);
            table.add_row(Row::Empty);
        }
        self.render_summary(&mut table, "Total (A+L)".into(), &total_al, false);

        table.add_row(Row::Separator);

        let mut total_eie = Position::default();
        for account_type in [Equity, Income, Expenses] {
            let header = account_type.to_string();
            let Some(node) = self.root.children.get(&header) else {
                continue;
            };
            node.iter_post().for_each(|(_, node)| total_eie += node);
            self.render_subtree(&mut table, node, header, true);
            table.add_row(Row::Empty);
        }
        self.render_summary(&mut table, "Total (E+I+E)".into(), &total_eie, true);

        table.add_row(Row::Separator);

        let mut delta = total_al;
        delta += &total_eie;
        self.render_summary(&mut table, "Delta".into(), &delta, false);
        table.add_row(Row::Separator);
        table
    }

    fn render_header(&self, table: &mut Table) {
        let mut cells = Vec::with_capacity(1 + self.dates.len());
        cells.push(Cell::Text {
            text: "Account".to_string(),
            align: Alignment::Center,
            indent: 0,
        });
        for date in &self.dates {
            cells.push(Cell::Text {
                text: format!("{}", date.format("%Y-%m-%d")),
                align: Alignment::Center,
                indent: 0,
            });
        }
        table.add_row(Row::Row(cells));
    }

    fn render_summary(&self, table: &mut Table, header: String, node: &Position, neg: bool) {
        self.render_node(table, header, 0, &node.values, neg);
    }

    fn render_subtree(&self, table: &mut Table, root: &Node<Position>, header: String, neg: bool) {
        root.iter_pre().for_each(|(path, node)| {
            let header = match path.last() {
                None => header.clone(),
                Some(segment) => segment.to_string(),
            };
            let indent = 2 * path.len();
            self.render_node(table, header, indent, &node.values, neg);
        });
    }

    fn render_node(
        &self,
        table: &mut Table,
        header: String,
        indent: usize,
        positions: &Positions<CommodityID, Positions<NaiveDate, Decimal>>,
        neg: bool,
    ) {
        let mut cells = Vec::with_capacity(1 + self.dates.len());
        cells.push(Cell::Text {
            text: header,
            indent,
            align: Alignment::Left,
        });
        let total_value = positions.values().sum::<Positions<NaiveDate, Decimal>>();
        for value in self.cumulative_sum(total_value) {
            if value.is_zero() {
                cells.push(Cell::Empty);
            } else {
                cells.push(Cell::Decimal {
                    value: if neg { -value } else { value },
                });
            }
        }
        table.add_row(Row::Row(cells));
    }

    fn cumulative_sum(&self, positions: Positions<NaiveDate, Decimal>) -> Vec<Decimal> {
        let mut sum = Decimal::ZERO;
        self.dates
            .iter()
            .map(|date| match (positions.get(date), self.cumulative) {
                (None, true) => sum,
                (None, false) => Decimal::ZERO,
                (Some(value), true) => {
                    sum += value;
                    sum
                }
                (Some(value), false) => *value,
            })
            .collect()
    }
}
