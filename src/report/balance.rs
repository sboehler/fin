use std::{
    cell::RefCell,
    collections::HashMap,
    fmt::Alignment,
    iter::{self},
    num::ParseIntError,
    ops::{Deref, Neg},
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

#[derive(Default)]
pub struct DatedPositions {
    positions: Positions<AccountID, Positions<CommodityID, Positions<NaiveDate, Decimal>>>,
}

impl DatedPositions {
    fn add_quantity(&mut self, row: Entry) {
        let position = self.positions.entry(row.account).or_default();
        position
            .entry(row.commodity)
            .or_default()
            .insert_or_add(row.date, &row.quantity);
    }

    fn add_value(&mut self, row: Entry) {
        let position = self.positions.entry(row.account).or_default();
        if let Some(value) = row.value {
            position
                .entry(row.commodity)
                .or_default()
                .insert_or_add(row.date, &value);
        }
    }
}

impl Deref for DatedPositions {
    type Target = Positions<AccountID, Positions<CommodityID, Positions<NaiveDate, Decimal>>>;

    fn deref(&self) -> &Self::Target {
        &self.positions
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
            Amount::Aggregate(values) => values.iter().map(|d| d * d).sum(),
            Amount::ByCommodity(v) => v.values().flat_map(|vs| vs.iter()).map(|d| d * d).sum(),
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
    Aggregate(Vec<Decimal>),
    ByCommodity(HashMap<String, Vec<Decimal>>),
}

impl Neg for Amount {
    type Output = Amount;

    fn neg(mut self) -> Self::Output {
        match &mut self {
            Amount::Empty => {}
            Amount::Aggregate(values) => {
                for value in values {
                    *value = -*value;
                }
            }
            Amount::ByCommodity(values) => {
                for (_, values) in values.iter_mut() {
                    for value in values {
                        *value = -*value;
                    }
                }
            }
        };
        self
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
            Amount::Aggregate(values) => {
                for value in values {
                    cells.push(Cell::Decimal { value: *value })
                }
                table.add_row(Row::Row(cells));
            }
            Amount::ByCommodity(values) => {
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
    pub report_amount: ReportAmount,
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
        let mut dated_positions = DatedPositions::default();
        let add = match self.report_amount {
            ReportAmount::Value => DatedPositions::add_value,
            ReportAmount::Quantity => DatedPositions::add_quantity,
        };
        for row in journal
            .query()
            .filter(|e| partition.contains(e.date))
            .flat_map(|row| closer.process(row))
            .flat_map(|row| aligner.align(row))
        {
            add(&mut dated_positions, row);
        }
        let dated_positions = self.shorten(journal, dated_positions);
        self.create_report(journal, dates, dated_positions)
    }

    fn shorten(&self, journal: &Journal, dated_positions: DatedPositions) -> DatedPositions {
        DatedPositions {
            positions: dated_positions.map_keys(|account| {
                let name = journal.registry().account_name(account);
                for mapping in &self.mapping {
                    if mapping.regex.is_match(&name) {
                        return journal.registry().shorten(account, mapping.level);
                    }
                }
                Some(account)
            }),
        }
    }

    fn create_report(
        &self,
        journal: &Journal,
        dates: Vec<NaiveDate>,
        dated_positions: DatedPositions,
    ) -> Report {
        let mut root: Node = Default::default();
        let mut total_al = Positions::default();
        let mut total_eie = Positions::default();

        dated_positions.iter().for_each(|(account, position)| {
            let account_name = journal.registry().account_name(*account);
            let segments = account_name.split(":").collect::<Vec<_>>();
            let show_commodities = self.show_commodities(journal.registry(), account);
            let mut value = self.to_amount(journal.registry(), &dates, position, show_commodities);
            if !account.account_type.is_al() {
                value = -value;
            }
            root.insert(&segments, value);
            match account.account_type {
                Assets | Liabilities => total_al += position,
                Expenses | Income | Equity => total_eie += position,
            }
        });

        let mut delta = Positions::default();
        delta += &total_al;
        delta += &total_eie;
        total_eie = total_eie.neg();

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
        position: &Positions<CommodityID, Positions<NaiveDate, Decimal>>,
        show_commodities: bool,
    ) -> Amount {
        match (&self.report_amount, show_commodities) {
            (ReportAmount::Value, false) => {
                let aggregate_positions = Self::aggregate_values(position);
                let aggregate_value = self.to_vector(dates, &aggregate_positions);
                Amount::Aggregate(aggregate_value)
            }
            _ => {
                let quantity_by_commodity = self.by_commodity_name(registry, dates, position);
                Amount::ByCommodity(quantity_by_commodity)
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

    fn aggregate_values(
        position: &Positions<CommodityID, Positions<NaiveDate, Decimal>>,
    ) -> Positions<NaiveDate, Decimal> {
        position.values().sum::<Positions<NaiveDate, Decimal>>()
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
