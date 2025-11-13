use std::{
    cell::RefCell,
    collections::HashMap,
    fmt::Alignment,
    iter::{self, Sum},
    num::ParseIntError,
    ops::{Add, AddAssign, Deref},
    rc::Rc,
    str::FromStr,
};

use chrono::NaiveDate;
use regex::Regex;
use rust_decimal::Decimal;

use crate::model::{
    entities::{AccountID, AccountType, CommodityID, Interval, Partition, Positions},
    journal::{Closer, Entry, Journal},
    registry::Registry,
};

use super::table::{Cell, Row, Table};

pub struct Aligner {
    dates: Vec<NaiveDate>,
}

impl Aligner {
    pub fn new(dates: Vec<NaiveDate>) -> Self {
        Self { dates }
    }

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

#[derive(Default)]
pub struct DatedPositions {
    positions: Positions<AccountID, Position>,
}

impl DatedPositions {
    fn add(&mut self, row: Entry) {
        let pos = self.positions.entry(row.account).or_default();
        pos.quantities
            .entry(row.commodity)
            .or_default()
            .insert_or_add(row.date, &row.quantity);
        if let Some(value) = row.value {
            pos.values
                .entry(row.commodity)
                .or_default()
                .insert_or_add(row.date, &value);
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

#[derive(Default)]
pub struct Position {
    quantities: Positions<CommodityID, Positions<NaiveDate, Decimal>>,
    values: Positions<CommodityID, Positions<NaiveDate, Decimal>>,
}

impl Position {
    pub fn negate(&mut self) {
        self.quantities.values_mut().for_each(|positions| {
            positions.values_mut().for_each(|value| *value = -*value);
        });
        self.values.values_mut().for_each(|positions| {
            positions.values_mut().for_each(|value| *value = -*value);
        });
    }
}

impl AddAssign<&Position> for Position {
    fn add_assign(&mut self, rhs: &Position) {
        self.quantities += &rhs.quantities;
        self.values += &rhs.values;
    }
}

impl Add<&Position> for &Position {
    type Output = Position;

    fn add(self, rhs: &Position) -> Position {
        Position {
            quantities: &self.quantities + &rhs.quantities,
            values: &self.values + &rhs.values,
        }
    }
}

#[derive(Default)]
struct Node {
    children: HashMap<String, Node>,
    amount: Amount,
    weight: RefCell<Decimal>,
}

impl Node {
    pub fn insert(&mut self, names: &[&str], amount: Amount) {
        match *names {
            [first, ref rest @ ..] => self
                .children
                .entry(first.into())
                .or_default()
                .insert(rest, amount),
            [] => self.amount = amount,
        }
    }

    pub fn update_weights(&self) -> Decimal {
        let child_weights: Decimal = self.children.values().map(Node::update_weights).sum();
        let local_weight: Decimal = match &self.amount {
            Amount::Empty => Decimal::ZERO,
            Amount::AggregateValue(values) => values.iter().map(|d| d * d).sum(),
            Amount::ValueByCommodity(v) | Amount::QuantityByCommodity(v) => {
                v.values().flat_map(|vs| vs.iter()).map(|d| d * d).sum()
            }
        };
        let weight = local_weight + child_weights;
        self.weight.replace(weight);
        weight
    }
}

#[derive(Default)]
enum Amount {
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
    dates: Vec<NaiveDate>,

    root: Node,

    total_al: Amount,
    total_eie: Amount,
    delta: Amount,
}

impl Report {
    pub fn render(&self) -> Table {
        let mut table = Table::new(
            iter::once(0)
                .chain(std::iter::repeat_n(1, self.dates.len()))
                .collect::<Vec<_>>(),
        );
        table.add_row(Row::Separator);
        self.render_header(&mut table);
        table.add_row(Row::Separator);

        for account_type in [Assets, Liabilities] {
            let header = account_type.to_string();
            let Some(node) = self.root.children.get(&header) else {
                continue;
            };
            node.update_weights();
            self.render_subtree(&mut table, node, header, 0);
            table.add_row(Row::Empty);
        }
        self.render_summary(&mut table, "Total (A+L)".into(), &self.total_al);

        table.add_row(Row::Separator);

        for account_type in [Equity, Income, Expenses] {
            let header = account_type.to_string();
            let Some(node) = self.root.children.get(&header) else {
                continue;
            };
            node.update_weights();
            self.render_subtree(&mut table, node, header, 0);
            table.add_row(Row::Empty);
        }
        self.render_summary(&mut table, "Total (E+I+E)".into(), &self.total_eie);

        table.add_row(Row::Separator);

        self.render_summary(&mut table, "Delta".into(), &self.delta);
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

    fn render_summary(&self, table: &mut Table, header: String, node: &Amount) {
        self.render_line(table, header, 0, node);
    }

    fn render_subtree(&self, table: &mut Table, root: &Node, header: String, indent: usize) {
        let mut children = root.children.iter().collect::<Vec<_>>();
        children.sort_by(|a, b| a.1.weight.borrow().cmp(&b.1.weight.borrow()).reverse());

        self.render_line(table, header, indent, &root.amount);
        for (segment, child) in children {
            self.render_subtree(table, child, segment.clone(), indent + 2);
        }
    }

    fn render_line(&self, table: &mut Table, header: String, indent: usize, amount: &Amount) {
        let mut cells = Vec::with_capacity(1 + self.dates.len());
        cells.push(Cell::Text {
            text: header,
            indent,
            align: Alignment::Left,
        });
        match amount {
            Amount::Empty => {
                for _ in &self.dates {
                    cells.push(Cell::Empty);
                }
                table.add_row(Row::Row(cells));
            }
            Amount::AggregateValue(values) => {
                for value in values {
                    cells.push(Cell::Decimal { value: *value })
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
    pub from: Option<NaiveDate>,
    pub to: NaiveDate,
    pub num_periods: Option<usize>,
    pub period: Interval,
    pub mapping: Vec<Mapping>,
    pub cumulative: bool,
    pub amount_type: ReportAmount,
    pub show_commodities: Vec<Regex>,
}

pub enum ReportAmount {
    Value,
    Quantity,
}

impl ReportBuilder {
    pub fn build(&self, journal: &Journal) -> Report {
        let from = self.from.or(journal.min_transaction_date()).unwrap();
        let partition = Partition::from_interval(from, self.to, self.period);
        let dates = partition
            .last_n(self.num_periods.map(|v| v + 1).unwrap_or(usize::MAX))
            .end_dates();
        let mut closer = Closer::new(
            partition.start_dates(),
            journal.registry().account_id("Equity:Equity").unwrap(),
            self.cumulative,
        );
        let aligner = Aligner::new(dates.clone());
        let dated_positions = journal
            .query()
            .filter(|e| partition.contains(e.date))
            .flat_map(|row| closer.process(row))
            .flat_map(|row| aligner.align(row))
            .sum::<DatedPositions>();
        let shortener = Shortener::new(
            journal.registry().clone(),
            self.mapping
                .iter()
                .map(|m| (m.regex.clone(), m.level))
                .collect(),
        );
        let dated_positions = dated_positions.map_keys(|account| shortener.shorten(account));

        let mut root: Node = Default::default();

        let mut total_al = Position::default();
        let mut total_eie = Position::default();

        dated_positions.iter().for_each(|(account, position)| {
            let account_name = journal.registry().account_name(*account);
            let segments = account_name.split(":").collect::<Vec<_>>();
            let show_commodities = self.show_commodities(journal.registry(), account);
            let mut value = self.to_amount(journal.registry(), &dates, position, show_commodities);
            if !account.account_type.is_al() {
                value.negate();
            }
            root.insert(&segments, value);

            match account.account_type {
                Assets | Liabilities => total_al += position,
                Expenses | Income | Equity => total_eie += position,
            }
        });

        let mut delta = Position::default();
        delta += &total_al;
        delta += &total_eie;
        total_eie.negate();

        let total_al = self.to_amount(journal.registry(), &dates, &total_al, false);
        let total_eie = self.to_amount(journal.registry(), &dates, &total_eie, false);
        let delta = self.to_amount(journal.registry(), &dates, &delta, false);

        Report {
            dates: dates.clone(),
            root,
            total_al,
            total_eie,
            delta,
        }
    }

    fn to_amount(
        &self,
        registry: &Rc<Registry>,
        dates: &[NaiveDate],
        position: &Position,
        show_commodities: bool,
    ) -> Amount {
        match self.amount_type {
            ReportAmount::Value if show_commodities => {
                let value_by_commodity = self.by_commodity_name(registry, dates, &position.values);
                Amount::ValueByCommodity(value_by_commodity)
            }
            ReportAmount::Value => {
                let aggregate_positions = Self::aggregate_values(position);
                let aggregate_value = self.to_vector(dates, &aggregate_positions);
                Amount::AggregateValue(aggregate_value)
            }
            ReportAmount::Quantity => {
                let quantity_by_commodity =
                    self.by_commodity_name(registry, dates, &position.quantities);
                Amount::QuantityByCommodity(quantity_by_commodity)
            }
        }
    }

    fn by_commodity_name(
        &self,
        registry: &Rc<Registry>,
        dates: &[NaiveDate],
        positions: &Positions<CommodityID, Positions<NaiveDate, Decimal>>,
    ) -> HashMap<String, Vec<Decimal>> {
        positions
            .iter()
            .map(|(commodity, positions)| {
                let name = registry.commodity_name(*commodity);
                let values = self.to_vector(dates, positions);
                (name, values)
            })
            .collect::<HashMap<_, _>>()
    }

    fn show_commodities(&self, registry: &Rc<Registry>, account: &AccountID) -> bool {
        let name = registry.account_name(*account);
        self.show_commodities.iter().any(|re| re.is_match(&name))
    }

    fn to_vector(
        &self,
        dates: &[NaiveDate],
        positions: &Positions<NaiveDate, Decimal>,
    ) -> Vec<Decimal> {
        let mut sum = Decimal::ZERO;
        dates
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

#[derive(Clone)]
pub struct Mapping {
    regex: Regex,
    level: usize,
}

impl FromStr for Mapping {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let mut parts = s.split(',');
        let levels = parts
            .next()
            .ok_or(format!("invalid mapping: {s}"))?
            .parse()
            .map_err(|e: ParseIntError| e.to_string())?;
        let regex = Regex::new(parts.next().unwrap_or(".*")).map_err(|e| e.to_string())?;
        Ok(Mapping {
            regex,
            level: levels,
        })
    }
}
