use std::{
    collections::HashMap,
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

#[derive(Default)]
pub enum Amount {
    #[default]
    Empty,
    AggregateValue(Vec<Decimal>),
    ValueByCommodity(HashMap<String, Vec<Decimal>>),
    QuantityByCommodity(HashMap<String, Vec<Decimal>>),
}

impl Amount {
    pub fn negate(&mut self) {
        match self {
            Amount::Empty => {}
            Amount::AggregateValue(values) => {
                for value in values {
                    *value = -*value;
                }
            }
            Amount::ValueByCommodity(values) => {
                for (_, values) in values.iter_mut() {
                    for value in values {
                        *value = -*value;
                    }
                }
            }
            Amount::QuantityByCommodity(values) => {
                for (_, values) in values.iter_mut() {
                    for value in values {
                        *value = -*value;
                    }
                }
            }
        }
    }
}

use AccountType::*;

pub struct Report {
    pub dates: Vec<NaiveDate>,

    pub root: Node<Amount>,

    pub total_al: Amount,
    pub total_eie: Amount,
    pub delta: Amount,
}

impl Report {
    pub fn render(&self) -> Table {
        let mut table = Table::new(
            iter::once(0)
                .chain(iter::repeat(1).take(self.dates.len()))
                .collect::<Vec<_>>(),
        );
        table.add_row(Row::Separator);
        self.render_header(&mut table);
        table.add_row(Row::Separator);

        // let mut total_al = Position::default();
        for account_type in [Assets, Liabilities] {
            let header = account_type.to_string();
            let Some(node) = self.root.children.get(&header) else {
                continue;
            };
            // node.iter_post().for_each(|(_, node)| total_al += node);
            self.render_subtree(&mut table, node, header);
            table.add_row(Row::Empty);
        }
        // self.render_summary(&mut table, "Total (A+L)".into(), &total_al, false);

        table.add_row(Row::Separator);

        // let mut total_eie = Position::default();
        for account_type in [Equity, Income, Expenses] {
            let header = account_type.to_string();
            let Some(node) = self.root.children.get(&header) else {
                continue;
            };
            // node.iter_post().for_each(|(_, node)| total_eie += node);
            self.render_subtree(&mut table, node, header);
            table.add_row(Row::Empty);
        }
        // self.render_summary(&mut table, "Total (E+I+E)".into(), &total_eie, true);

        table.add_row(Row::Separator);

        // let mut delta = total_al;
        // delta += &total_eie;
        // self.render_summary(&mut table, "Delta".into(), &delta, false);
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

    // fn render_summary(
    //     &self,
    //     table: &mut Table,
    //     header: String,
    //     node: &RenderedPosition,
    //     neg: bool,
    // ) {
    //     self.render_node(table, header, 0, node, neg);
    // }

    fn render_subtree(&self, table: &mut Table, root: &Node<Amount>, header: String) {
        root.iter_pre().for_each(|(path, node)| {
            let header = match path.last() {
                None => header.clone(),
                Some(segment) => segment.to_string(),
            };
            let indent = 2 * path.len();
            self.render_node(table, header, indent, node);
        });
    }

    fn render_node(&self, table: &mut Table, header: String, indent: usize, node: &Amount) {
        let mut cells = Vec::with_capacity(1 + self.dates.len());
        cells.push(Cell::Text {
            text: header,
            indent,
            align: Alignment::Left,
        });
        match node {
            Amount::Empty => {
                for _ in &self.dates {
                    cells.push(Cell::Empty);
                }
                table.add_row(Row::Row(cells));
            }
            Amount::AggregateValue(values) => {
                for value in values {
                    if value.is_zero() {
                        cells.push(Cell::Empty);
                    } else {
                        cells.push(Cell::Decimal { value: *value });
                    }
                }
                table.add_row(Row::Row(cells));
            }
            Amount::ValueByCommodity(values) => {
                for _ in &self.dates {
                    cells.push(Cell::Empty);
                }
                table.add_row(Row::Row(cells));
                for (commodity, values) in values.iter() {
                    let mut cells = Vec::with_capacity(1 + self.dates.len());
                    cells.push(Cell::Text {
                        text: commodity.clone(),
                        indent: indent + 2,
                        align: Alignment::Left,
                    });
                    for value in values {
                        cells.push(Cell::Decimal { value: *value });
                    }
                    table.add_row(Row::Row(cells))
                }
            }
            Amount::QuantityByCommodity(_) => todo!(),
        }
    }
}

pub struct ReportBuilder {
    pub dates: Vec<NaiveDate>,
    pub registry: Rc<Registry>,
    pub cumulative: bool,
    pub amount_type: ReportAmount,
    pub show_commodities: Vec<Regex>,
}

pub enum ReportAmount {
    Value,
    Quantity,
}

impl ReportBuilder {
    pub fn build(&self, dated_positions: &DatedPositions) -> Report {
        let mut root: Node<Amount> = Default::default();

        let mut total_al = Position::default();
        let mut total_eie = Position::default();

        dated_positions.iter().for_each(|(account, position)| {
            let account_name = self.registry.account_name(*account);
            let segments = self.to_segments(&account_name);
            let mut value = match self.amount_type {
                ReportAmount::Value if self.show_commodities(account) => {
                    let value_by_commodity = self.by_commodity_name(&position.values);
                    Amount::ValueByCommodity(value_by_commodity)
                }
                ReportAmount::Value => {
                    let aggregate_positions = Self::aggregate_values(position);
                    let aggregate_value = self.to_vector(&aggregate_positions);
                    Amount::AggregateValue(aggregate_value)
                }
                ReportAmount::Quantity => {
                    let quantity_by_commodity = self.by_commodity_name(&position.quantities);
                    Amount::QuantityByCommodity(quantity_by_commodity)
                }
            };
            if !account.account_type.is_al() {
                value.negate();
            }
            root.insert(&segments, value);

            match account.account_type {
                Assets | Liabilities => total_al += position,
                Expenses | Income | Equity => total_eie += position,
            }
        });

        Report {
            dates: self.dates.clone(),
            root,
            total_al: Amount::Empty,
            total_eie: Amount::Empty,
            delta: Amount::Empty,
        }
    }

    fn to_segments<'a>(&self, account_name: &'a str) -> Vec<&'a str> {
        account_name.split(":").collect::<Vec<_>>()
    }

    fn by_commodity_name(
        &self,
        positions: &Positions<CommodityID, Positions<NaiveDate, Decimal>>,
    ) -> HashMap<String, Vec<Decimal>> {
        positions
            .iter()
            .map(|(commodity, positions)| {
                let name = self.registry.commodity_name(*commodity);
                let values = self.to_vector(positions);
                (name, values)
            })
            .collect::<HashMap<_, _>>()
    }

    fn show_commodities(&self, account: &AccountID) -> bool {
        let name = self.registry.account_name(*account);
        self.show_commodities.iter().any(|re| re.is_match(&name))
    }

    fn to_vector(&self, positions: &Positions<NaiveDate, Decimal>) -> Vec<Decimal> {
        let mut sum = Decimal::ZERO;
        self.dates
            .iter()
            .map(|date| positions.get(date).cloned().unwrap_or_default())
            .map(|value| {
                if self.cumulative {
                    sum += value;
                    sum
                } else {
                    value
                }
            })
            .collect()
    }

    fn aggregate_values(position: &Position) -> Positions<NaiveDate, Decimal> {
        position
            .values
            .values()
            .sum::<Positions<NaiveDate, Decimal>>()
    }
}
