use std::{io::Write, rc::Rc};

use super::{
    entities::{Assertion, Price, Transaction},
    registry::Registry,
};
use crate::syntax::{
    arrows::{self, DEFAULT_WIDTH, Flow},
    format::format_file,
    parse_text,
};

pub struct Printer<'a, W: Write> {
    registry: Rc<Registry>,
    writer: &'a mut W,
}

impl<'a, W: Write> Printer<'a, W> {
    pub fn new(writer: &'a mut W, registry: Rc<Registry>) -> Self {
        Self { registry, writer }
    }

    pub fn price(&mut self, p: &Price) -> std::io::Result<()> {
        writeln!(
            self.writer,
            "{date} price {commodity} {price} {target}",
            date = p.date,
            commodity = self.registry.commodity_name(p.commodity),
            price = p.price,
            target = self.registry.commodity_name(p.target),
        )
    }

    /// Prints the transactions in the arrow notation, then the assertions
    /// below them, separated by blank lines and laid out by the formatter so
    /// that what is imported reads as `fin format` would write it.
    pub fn journal(&mut self, ts: &[Transaction], assertions: &[Assertion]) -> std::io::Result<()> {
        let mut blocks = ts.iter().map(|t| self.transaction(t)).collect::<Vec<_>>();
        if !assertions.is_empty() {
            // The assertions are one block: the formatter writes each of them
            // as a group, and the ones which share a date as one directive.
            blocks.push(assertions.iter().map(|a| self.assertion(a)).collect());
        }
        let text = blocks.join("\n");
        // Formatting means parsing, which is also what says that the printer
        // wrote a journal rather than something merely journal-shaped.
        let tree = parse_text(&text).map_err(|e| {
            std::io::Error::other(format!("printed a journal which does not parse: {e}"))
        })?;
        format_file(self.writer, &text, &tree)
    }

    /// One assertion, on the single line the formatter reads it from.
    fn assertion(&self, a: &Assertion) -> String {
        format!(
            "{date} balance {account} {balance} {commodity}\n",
            date = a.date,
            account = self.registry.account_name(a.account),
            balance = a.balance,
            commodity = self.registry.commodity_name(a.commodity),
        )
    }

    /// One transaction, as the lines it is written on.
    fn transaction(&self, t: &Transaction) -> String {
        let addon = t.targets.as_ref().map(|targets| {
            let names = targets
                .iter()
                .map(|c| self.registry.commodity_name(*c))
                .collect::<Vec<_>>();
            format!("@performance({})", names.join(","))
        });
        let bookings = self.bookings(t);
        let flows = bookings
            .iter()
            .map(|[credit, debit, quantity, commodity]| {
                Flow::new(credit, debit, quantity, commodity)
            })
            .collect();
        let date = t.date.to_string();
        let lines = arrows::transaction(
            addon.as_deref(),
            &date,
            &t.description,
            flows,
            DEFAULT_WIDTH,
        )
        // The arrow notation writes the description below the date and so
        // needs one: a transaction without it keeps the older notation.
        .unwrap_or_else(|| quoted(addon.as_deref(), &date, &t.description, &bookings));
        lines.join("\n") + "\n"
    }

    /// The bookings of the transaction as `[credit, debit, quantity,
    /// commodity]`. They come in (credit, debit) pairs as produced by
    /// `Booking::create`, so each pair is read from its debit side.
    fn bookings(&self, t: &Transaction) -> Vec<[String; 4]> {
        t.bookings
            .iter()
            .skip(1)
            .step_by(2)
            .map(|b| {
                [
                    self.registry.account_name(b.other),
                    self.registry.account_name(b.account),
                    b.quantity.to_string(),
                    self.registry.commodity_name(b.commodity),
                ]
            })
            .collect()
    }
}

/// A transaction in the older notation: the description quoted on the date
/// line, and one line per booking naming both of its accounts.
fn quoted(
    addon: Option<&str>,
    date: &str,
    description: &str,
    bookings: &[[String; 4]],
) -> Vec<String> {
    let mut lines = Vec::new();
    lines.extend(addon.map(str::to_string));
    lines.push(format!("{date} \"{description}\""));
    for [credit, debit, quantity, commodity] in bookings {
        lines.push(format!("{credit} {debit} {quantity} {commodity}"));
    }
    lines
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use chrono::NaiveDate;
    use pretty_assertions::assert_eq;
    use rust_decimal::Decimal;

    use super::*;
    use crate::model::entities::Booking;
    use crate::model::registry::Registry;

    /// One booking, as `(credit, debit, quantity, commodity)`.
    type Booked<'a> = (&'a str, &'a str, &'a str, &'a str);

    /// One assertion, as `(date, account, balance, commodity)`.
    type Asserted<'a> = (&'a str, &'a str, &'a str, &'a str);

    /// The journal the transactions and assertions are printed to, built as
    /// the importers build theirs.
    fn printed(ts: &[(&str, &[Booked])], asserted: &[Asserted]) -> String {
        let registry = Rc::new(Registry::new());
        let transactions = ts
            .iter()
            .map(|(description, bookings)| Transaction {
                loc: None,
                date: NaiveDate::from_ymd_opt(2026, 6, 24).unwrap(),
                description: Rc::new(description.to_string()),
                bookings: bookings
                    .iter()
                    .flat_map(|(credit, debit, quantity, commodity)| {
                        Booking::create(
                            registry.account_id(credit).unwrap(),
                            registry.account_id(debit).unwrap(),
                            Decimal::from_str_exact(quantity).unwrap(),
                            registry.commodity_id(commodity).unwrap(),
                            None,
                        )
                    })
                    .collect(),
                targets: None,
            })
            .collect::<Vec<_>>();
        let assertions = asserted
            .iter()
            .map(|(date, account, balance, commodity)| Assertion {
                loc: None,
                date: date.parse().unwrap(),
                account: registry.account_id(account).unwrap(),
                balance: Decimal::from_str_exact(balance).unwrap(),
                commodity: registry.commodity_id(commodity).unwrap(),
            })
            .collect::<Vec<_>>();
        let mut out = Vec::new();
        Printer::new(&mut out, registry)
            .journal(&transactions, &assertions)
            .expect("prints");
        String::from_utf8(out).unwrap()
    }

    /// A booking between an account and a category is one group, written
    /// around the account, with the arrow saying where the money went.
    #[test]
    fn prints_the_arrow_notation() {
        assert_eq!(
            "\
2026-06-24
  Groceries
Assets:Bank
-> Expenses:Food      42.50 CHF
",
            printed(
                &[(
                    "Groceries",
                    &[("Assets:Bank", "Expenses:Food", "42.50", "CHF")]
                )],
                &[]
            )
        );
    }

    /// A long description is wrapped rather than run off the page.
    #[test]
    fn wraps_long_descriptions() {
        let description = "A description long enough that it has to be wrapped \
                           over two lines, and then some";
        assert_eq!(
            "\
2026-06-24
  A description long enough that it has to be wrapped over two lines, and then
  some
Assets:Bank
-> Expenses:Food          1 CHF
",
            printed(
                &[(description, &[("Assets:Bank", "Expenses:Food", "1", "CHF")])],
                &[]
            )
        );
    }

    /// The arrow notation writes the description below the date and so needs
    /// one; a transaction without it keeps the older notation, which the
    /// journal still allows.
    #[test]
    fn keeps_transactions_without_a_description() {
        assert_eq!(
            "\
2026-06-24 \"\"
Assets:Bank   Expenses:Food          1 CHF

2026-06-24
  Groceries
Assets:Bank
-> Expenses:Food      42.50 CHF
",
            printed(
                &[
                    ("", &[("Assets:Bank", "Expenses:Food", "1", "CHF")]),
                    (
                        "Groceries",
                        &[("Assets:Bank", "Expenses:Food", "42.50", "CHF")]
                    ),
                ],
                &[]
            )
        );
    }

    /// The assertions are printed below the transactions, as the groups the
    /// formatter writes them as: their amounts in the column the amounts of
    /// the transactions are in, and the ones on a date as one directive.
    #[test]
    fn prints_the_assertions_below_the_transactions() {
        assert_eq!(
            "\
2026-06-24
  Groceries
Assets:Bank
-> Expenses:Food      42.50 CHF

2026-06-24 balance
Assets:Bank          100.00 CHF
Assets:Bank           50.00 EUR

2026-06-25 balance
Assets:Bank          120.00 CHF
",
            printed(
                &[(
                    "Groceries",
                    &[("Assets:Bank", "Expenses:Food", "42.50", "CHF")]
                )],
                &[
                    ("2026-06-24", "Assets:Bank", "100.00", "CHF"),
                    ("2026-06-24", "Assets:Bank", "50.00", "EUR"),
                    ("2026-06-25", "Assets:Bank", "120.00", "CHF"),
                ]
            )
        );
    }
}
