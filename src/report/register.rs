//! A chronological listing of the bookings of a group of accounts.
//!
//! The group is every account at or below an account prefix, so
//! `Assets:Bank` covers `Assets:Bank:Checking` and `Assets:Bank:Savings`.
//! One row is one booking of one of those accounts: the account facing it and
//! what moved. The group is the thing being looked at, so what is worth
//! naming is the other end of the booking.
//!
//! Movements inside the group are left out: a transfer between two accounts
//! of the group is not money arriving or leaving, and the two rows it would
//! produce cancel out anyway.

use std::fmt::Alignment;

use chrono::NaiveDate;
use rust_decimal::Decimal;

use crate::model::{
    entities::{AccountID, CommodityID, Period},
    journal::Journal,
    registry::Registry,
};

use super::mapping::AccountMapper;
use super::table::{Cell, Row, Table};

pub struct Register {
    rows: Vec<RegisterRow>,
}

struct RegisterRow {
    date: NaiveDate,
    /// Already wrapped: the line breaks are the ones to print.
    description: String,
    /// The account facing the one the booking belongs to.
    account: String,
    amount: Decimal,
    commodity: String,
}

impl Register {
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The number of decimal places the amounts need to be shown exactly.
    pub fn scale(&self) -> usize {
        self.rows
            .iter()
            .map(|row| row.amount.scale() as usize)
            .max()
            .unwrap_or(0)
    }

    pub fn to_table(&self) -> Table {
        // Every column has its own width.
        let mut table = Table::new((0..5).collect());
        table.add_row(Row::Separator);
        table.add_row(Row::Row(
            ["Date", "Description", "Account", "Amount", "Commodity"]
                .iter()
                .map(|header| Cell::Text {
                    text: header.to_string(),
                    align: Alignment::Center,
                    indent: 0,
                })
                .collect(),
        ));
        table.add_row(Row::Separator);
        for row in &self.rows {
            table.add_row(Row::Row(vec![
                text(row.date.format("%Y-%m-%d").to_string()),
                text(row.description.clone()),
                text(row.account.clone()),
                Cell::Decimal { value: row.amount },
                text(row.commodity.clone()),
            ]));
        }
        table.add_row(Row::Separator);
        table
    }
}

fn text(text: String) -> Cell {
    Cell::Text {
        text,
        align: Alignment::Left,
        indent: 0,
    }
}

pub struct RegisterBuilder {
    pub from: Option<NaiveDate>,
    pub to: NaiveDate,
    /// The account prefix the group is made of. An empty prefix is every
    /// account.
    pub account: String,
    pub mapper: AccountMapper,
    /// The commodity the amounts are shown in. Without one they are the
    /// quantities booked, in the commodity they were booked in.
    pub valuation: Option<CommodityID>,
    /// The width the description is wrapped to.
    pub width: usize,
}

impl RegisterBuilder {
    pub fn build(&self, journal: &Journal) -> Register {
        let registry = journal.registry();
        let from = self
            .from
            .or_else(|| journal.min_transaction_date())
            .unwrap_or(self.to);
        let period = Period(from, self.to);
        // `query` yields the days in order, so the rows come out by date
        // and, within a date, in the order the journal has them.
        let rows = journal
            .query()
            .filter(|entry| {
                period.contains(entry.date)
                    && self.selects(registry, entry.account)
                    // A booking with both ends in the group is a movement
                    // inside it, which the group as a whole did not make.
                    && !self.selects(registry, entry.other)
            })
            .filter_map(|entry| {
                let (amount, commodity) = match self.valuation {
                    Some(commodity) => (entry.value.unwrap_or_default(), commodity),
                    None => (entry.quantity, entry.commodity),
                };
                Some(RegisterRow {
                    date: entry.date,
                    description: wrap(&entry.description, self.width),
                    // An account the mapping drops takes the bookings facing
                    // it with it, which is what it does in the other reports
                    // too.
                    account: self.account_name(registry, entry.other)?,
                    amount,
                    commodity: registry.commodity_name(commodity),
                })
            })
            .collect();
        Register { rows }
    }

    /// Whether `account` is in the group, i.e. is the prefix itself or sits
    /// below it. The prefix is matched segment-wise, so `Assets:Bank` does
    /// not select `Assets:Bankrupt`.
    fn selects(&self, registry: &Registry, account: AccountID) -> bool {
        if self.account.is_empty() {
            return true;
        }
        registry
            .account_name(account)
            .strip_prefix(&self.account)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(':'))
    }

    fn account_name(&self, registry: &Registry, account: AccountID) -> Option<String> {
        self.mapper
            .map(registry, account)
            .map(|account| registry.account_name(account))
    }
}

/// Wraps `text` to lines of at most `width` characters, greedily. Line breaks
/// the description already has are kept, so a description written on several
/// lines stays on several lines. A word too long to fit is broken.
fn wrap(text: &str, width: usize) -> String {
    let width = width.max(1);
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            for chunk in chunks(word, width) {
                let length = line.chars().count();
                if length == 0 {
                    line.push_str(chunk);
                } else if length + 1 + chunk.chars().count() <= width {
                    line.push(' ');
                    line.push_str(chunk);
                } else {
                    lines.push(std::mem::take(&mut line));
                    line.push_str(chunk);
                }
            }
        }
        lines.push(line);
    }
    lines.join("\n")
}

/// `word` cut into pieces of at most `width` characters. A word which fits is
/// one piece.
fn chunks(word: &str, width: usize) -> Vec<&str> {
    if word.chars().count() <= width {
        return vec![word];
    }
    let mut chunks = Vec::new();
    let mut start = 0;
    let mut length = 0;
    for (i, _) in word.char_indices() {
        if length == width {
            chunks.push(&word[start..i]);
            start = i;
            length = 0;
        }
        length += 1;
    }
    chunks.push(&word[start..]);
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        model::build_journal,
        syntax::{parse_text, sourcefile::SourceFile},
    };
    use pretty_assertions::assert_eq;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    const JOURNAL: &str = "2025-01-01 open Assets:Bank:Checking
2025-01-01 open Assets:Bank:Savings
2025-01-01 open Assets:Bankrupt
2025-01-01 open Income:Salary
2025-01-01 open Expenses:Food:Groceries

2025-01-25
  Salary January
Assets:Bank:Checking
<- Income:Salary                6500.00 CHF

2025-01-31
  Groceries
Assets:Bank:Checking
-> Expenses:Food:Groceries       420.50 CHF

2025-02-05
  Transfer to savings
Assets:Bank:Checking
-> Assets:Bank:Savings          2000.00 CHF

2025-02-06
  Write-off
Assets:Bankrupt
<- Income:Salary                   1.00 CHF
";

    fn journal(text: &str, valuation: Option<&str>) -> Journal {
        let files = vec![(
            parse_text(text).unwrap(),
            SourceFile {
                path: None,
                text: text.to_string(),
            },
        )];
        let mut journal = build_journal(&files).unwrap();
        journal.check().unwrap();
        let valuation = valuation.map(|c| journal.registry().commodity_id(c).unwrap());
        journal.process(valuation).unwrap();
        journal
    }

    fn builder(account: &str) -> RegisterBuilder {
        RegisterBuilder {
            from: None,
            to: date(2025, 12, 31),
            account: account.to_string(),
            mapper: AccountMapper::default(),
            valuation: None,
            width: 80,
        }
    }

    /// One line per row, so that a difference is readable.
    fn rows(builder: &RegisterBuilder, journal: &Journal) -> Vec<String> {
        builder
            .build(journal)
            .rows
            .iter()
            .map(|row| {
                format!(
                    "{} {} {} {} {}",
                    row.date, row.description, row.account, row.amount, row.commodity
                )
            })
            .collect()
    }

    /// Every booking of the group is listed, with the account facing it.
    /// The transfer from `Assets:Bank:Checking` to `Assets:Bank:Savings` is
    /// a movement inside the group and is left out. The prefix is matched
    /// segment-wise, so the write-off of `Assets:Bankrupt` is not under
    /// `Assets:Bank`.
    #[test]
    fn test_selects_the_group() {
        assert_eq!(
            rows(&builder("Assets:Bank"), &journal(JOURNAL, None)),
            vec![
                "2025-01-25 Salary January Income:Salary 6500.00 CHF",
                "2025-01-31 Groceries Expenses:Food:Groceries -420.50 CHF",
            ]
        );
    }

    /// A single account is a group of one, and a transfer to a sibling
    /// account leaves that group.
    #[test]
    fn test_selects_a_leaf_account() {
        assert_eq!(
            rows(&builder("Assets:Bank:Savings"), &journal(JOURNAL, None)),
            vec!["2025-02-05 Transfer to savings Assets:Bank:Checking 2000.00 CHF"]
        );
    }

    /// An account that does not exist selects nothing.
    #[test]
    fn test_unknown_prefix_is_empty() {
        assert!(
            builder("Assets:Bnk")
                .build(&journal(JOURNAL, None))
                .is_empty()
        );
    }

    /// `--from` and `--to` cut the listing down to the period.
    #[test]
    fn test_period() {
        let mut builder = builder("Assets:Bank");
        builder.from = Some(date(2025, 1, 31));
        builder.to = date(2025, 1, 31);
        assert_eq!(
            rows(&builder, &journal(JOURNAL, None)),
            vec!["2025-01-31 Groceries Expenses:Food:Groceries -420.50 CHF"]
        );
    }

    /// `-m` shortens the accounts shown.
    #[test]
    fn test_mapping() {
        let mut builder = builder("Assets:Bank:Checking");
        builder.mapper = AccountMapper::new(
            journal(JOURNAL, None).registry(),
            vec!["2,Expenses".parse().unwrap()],
            &[],
        )
        .unwrap();
        assert_eq!(
            rows(&builder, &journal(JOURNAL, None)),
            vec![
                "2025-01-25 Salary January Income:Salary 6500.00 CHF",
                "2025-01-31 Groceries Expenses:Food -420.50 CHF",
                "2025-02-05 Transfer to savings Assets:Bank:Savings -2000.00 CHF",
            ]
        );
    }

    /// Selection happens on the real account names, so a mapping cannot
    /// shorten an account into or out of the group: `Assets:Bank:Savings`
    /// stays out of a group of `Assets:Bank:Checking` even though both are
    /// shown as `Assets:Bank`.
    #[test]
    fn test_mapping_does_not_change_the_group() {
        let mut builder = builder("Assets:Bank:Checking");
        builder.mapper = AccountMapper::new(
            journal(JOURNAL, None).registry(),
            vec!["2".parse().unwrap()],
            &[],
        )
        .unwrap();
        assert_eq!(
            rows(&builder, &journal(JOURNAL, None)),
            vec![
                "2025-01-25 Salary January Income:Salary 6500.00 CHF",
                "2025-01-31 Groceries Expenses:Food -420.50 CHF",
                "2025-02-05 Transfer to savings Assets:Bank -2000.00 CHF",
            ]
        );
    }

    /// With a valuation the amounts are values in that commodity, and the
    /// value adjustments it books show up as rows of their own.
    #[test]
    fn test_valuation() {
        let text = format!(
            "{JOURNAL}\n2025-01-01 open Assets:Bank:Dollars\n2025-01-01 open Equity:Equity\n2025-01-01 price USD 0.80 CHF\n2025-02-01 price USD 0.90 CHF\n\n2025-01-25\n  Buy dollars\nAssets:Bank:Dollars\n<- Equity:Equity 100.00 USD\n"
        );
        let journal = journal(&text, Some("CHF"));
        let mut builder = builder("Assets:Bank:Dollars");
        builder.valuation = Some(journal.registry().commodity_id("CHF").unwrap());
        assert_eq!(
            rows(&builder, &journal),
            vec![
                "2025-01-25 Buy dollars Equity:Equity 80.0000 CHF",
                "2025-02-01 Adjust value of USD in account Assets:Bank:Dollars Income:Bank:Dollars 10.0000 CHF",
            ]
        );
    }

    /// Without a valuation the amounts are the quantities booked, in the
    /// commodity they were booked in.
    #[test]
    fn test_quantities_keep_their_commodity() {
        let text = format!(
            "{JOURNAL}\n2025-01-01 open Assets:Bank:Dollars\n2025-01-01 open Equity:Equity\n\n2025-01-25\n  Buy dollars\nAssets:Bank:Dollars\n<- Equity:Equity 100.00 USD\n"
        );
        assert_eq!(
            rows(&builder("Assets:Bank:Dollars"), &journal(&text, None)),
            vec!["2025-01-25 Buy dollars Equity:Equity 100.00 USD"]
        );
    }

    /// The scale is the one the amounts need, so that no digit is lost when
    /// the renderer rounds to it.
    #[test]
    fn test_scale() {
        assert_eq!(
            builder("Assets:Bank")
                .build(&journal(JOURNAL, None))
                .scale(),
            2
        );
        assert_eq!(builder("Nothing").build(&journal(JOURNAL, None)).scale(), 0);
    }

    #[test]
    fn test_wrap_fits() {
        assert_eq!(wrap("one two three", 20), "one two three");
    }

    #[test]
    fn test_wrap_breaks_between_words() {
        assert_eq!(wrap("one two three four", 9), "one two\nthree\nfour");
    }

    /// A word which cannot fit is broken rather than left to widen the
    /// column.
    #[test]
    fn test_wrap_breaks_a_long_word() {
        assert_eq!(wrap("aaaaaaa", 3), "aaa\naaa\na");
        assert_eq!(wrap("to aaaaaaa", 3), "to\naaa\naaa\na");
    }

    /// Characters, not bytes.
    #[test]
    fn test_wrap_counts_characters() {
        assert_eq!(wrap("Gebühren Zürich", 8), "Gebühren\nZürich");
    }

    /// The line breaks of a multi-line description are kept.
    #[test]
    fn test_wrap_keeps_line_breaks() {
        assert_eq!(
            wrap("Buy 11 VT\nat 154.45 USD", 80),
            "Buy 11 VT\nat 154.45 USD"
        );
    }

    #[test]
    fn test_wrap_empty() {
        assert_eq!(wrap("", 80), "");
    }
}
