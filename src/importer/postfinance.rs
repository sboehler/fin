use std::{
    borrow::Cow, collections::HashMap, error::Error, io::Write, iter::Peekable, path::PathBuf,
    rc::Rc,
};

use chrono::NaiveDate;
use clap::Args;
use csv::{StringRecord, StringRecordsIntoIter};
use rust_decimal::Decimal;

use crate::model::{
    entities::{Assertion, Booking, Transaction},
    printer::Printer,
    registry::Registry,
};

/// The account which receives the counter-postings of all imported
/// transactions. The user is expected to replace it when reconciling
/// the imported journal.
const TBD_ACCOUNT: &str = "Expenses:TBD";

/// Currency assumed when the statement does not declare one.
const DEFAULT_CURRENCY: &str = "CHF";

#[derive(Args)]
pub struct Command {
    source: PathBuf,

    #[arg(short, long)]
    account: String,
}

impl Command {
    pub fn run(&self, w: &mut impl Write) -> Result<(), Box<dyn Error>> {
        let source = std::fs::read(&self.source)?;
        import(&source, &self.account, w)
    }
}

/// Imports a Postfinance CSV account statement and writes the resulting
/// journal to `w`: one transaction per booking line, sorted by date, and a
/// balance assertion for the final balance if the statement reports one.
///
/// Statements are expected to be UTF-8 (with an optional BOM); Latin-1
/// encoded files are decoded as a fallback.
pub fn import(source: &[u8], account: &str, w: &mut impl Write) -> Result<(), Box<dyn Error>> {
    let registry = Rc::new(Registry::new());
    let account = registry.account_id(account)?;
    let tbd = registry.account_id(TBD_ACCOUNT)?;

    let statement = Statement::parse(&decode(source))?;
    let currency = registry.commodity_id(statement.currency())?;

    let assertion = statement.final_balance().map(|(date, balance)| Assertion {
        loc: None,
        date,
        account,
        balance,
        commodity: currency,
    });
    let mut transactions = statement
        .lines
        .into_iter()
        .map(|line| Transaction {
            loc: None,
            date: line.date,
            description: Rc::new(line.description),
            bookings: Booking::create(tbd, account, line.quantity, currency, None),
            targets: None,
        })
        .collect::<Vec<_>>();
    transactions.sort_by_key(|t| t.date);

    let mut printer = Printer::new(w, registry);
    printer.transactions(&transactions)?;
    if let Some(assertion) = assertion {
        printer.newline()?;
        printer.assertion(&assertion)?;
    }
    Ok(())
}

fn decode(bytes: &[u8]) -> Cow<'_, str> {
    match std::str::from_utf8(bytes) {
        Ok(s) => Cow::Borrowed(s),
        Err(_) => Cow::Owned(bytes.iter().map(|&b| b as char).collect()),
    }
}

#[derive(Debug)]
struct Statement {
    preamble: HashMap<String, String>,
    lines: Vec<Line>,
}

impl Statement {
    fn parse(source: &str) -> Result<Statement, Box<dyn Error>> {
        let mut parser = Parser::new(source);
        let preamble = parser.read_preamble()?;
        parser.read_header()?;
        let lines = parser.read_lines()?;
        parser.read_trailer()?;
        Ok(Statement { preamble, lines })
    }

    fn currency(&self) -> &str {
        self.preamble
            .get("Währung:")
            .map(String::as_str)
            .unwrap_or(DEFAULT_CURRENCY)
    }

    /// Returns the date and balance after the chronologically last booking
    /// line which reports a balance. The file order is detected from the
    /// dates rather than assumed, in case it is ever reversed.
    fn final_balance(&self) -> Option<(NaiveDate, Decimal)> {
        let descending = self.lines.windows(2).all(|w| w[0].date >= w[1].date);
        let mut with_balance = self
            .lines
            .iter()
            .filter_map(|l| l.balance.map(|b| (l.date, b)));
        if descending {
            with_balance.next()
        } else {
            with_balance.next_back()
        }
    }
}

/// A parsed booking line.
#[derive(Debug, PartialEq, Eq)]
struct Line {
    date: NaiveDate,
    description: String,
    /// Positive for credits, negative for debits.
    quantity: Decimal,
    /// The account balance after this booking, if reported.
    balance: Option<Decimal>,
}

/// Columns of a booking line, in file order. The header names depend on the
/// account currency ("Gutschrift in CHF" vs "Gutschrift in EUR"), so the
/// columns are addressed by position rather than by name.
#[derive(Clone, Copy)]
#[allow(dead_code)]
enum Field {
    Date,
    Description,
    Credit,
    Debit,
    Label,
    Category,
    Valuta,
    Balance,
}

impl Field {
    /// Reads this column from a 7- or 8-column record; missing columns read as empty.
    fn read(self, rec: &StringRecord) -> &str {
        rec.get(self as usize).unwrap_or_default()
    }
}

impl Line {
    fn parse(rec: &StringRecord) -> Result<Line, Box<dyn Error>> {
        let date = Field::Date.read(rec);
        let date = NaiveDate::parse_from_str(date, "%d.%m.%Y")
            .map_err(|e| format!("invalid date {date:?}: {e}"))?;
        let description = [Field::Description, Field::Category, Field::Label]
            .into_iter()
            .map(|f| f.read(rec))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
            // Descriptions are printed as quoted strings, which cannot contain quotes.
            .replace('"', "'");
        let quantity = parse_quantity(Field::Credit.read(rec), Field::Debit.read(rec))?;
        let balance = match Field::Balance.read(rec) {
            "" => None,
            s => Some(parse_decimal(s)?),
        };
        Ok(Line {
            date,
            description,
            quantity,
            balance,
        })
    }
}

/// Exactly one of the credit / debit fields must be set. Debits are already
/// negative in the statement, so the value is used verbatim.
fn parse_quantity(credit: &str, debit: &str) -> Result<Decimal, Box<dyn Error>> {
    match (credit, debit) {
        (c, "") if !c.is_empty() => parse_decimal(c),
        ("", d) if !d.is_empty() => parse_decimal(d),
        _ => Err(format!("invalid amount fields {credit:?} {debit:?}").into()),
    }
}

/// Parses an amount with optional thousands separators, e.g. `1'234.50`.
fn parse_decimal(s: &str) -> Result<Decimal, Box<dyn Error>> {
    Ok(s.replace('\'', "").parse()?)
}

/// Reads the statement's records section by section. Sections are told
/// apart by their number of fields, since the CSV has no other structure.
struct Parser<'a> {
    records: Peekable<StringRecordsIntoIter<&'a [u8]>>,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            records: csv::ReaderBuilder::new()
                .has_headers(false)
                .flexible(true)
                .delimiter(b';')
                .trim(csv::Trim::All)
                .from_reader(source.as_bytes())
                .into_records()
                .peekable(),
        }
    }

    fn next_record(&mut self) -> Result<Option<StringRecord>, csv::Error> {
        self.records.next().transpose()
    }

    /// Returns the next record if it satisfies `pred`, without consuming it
    /// otherwise.
    fn next_record_if(
        &mut self,
        pred: impl Fn(&StringRecord) -> bool,
    ) -> Result<Option<StringRecord>, csv::Error> {
        match self.records.peek() {
            Some(Ok(rec)) if !pred(rec) => Ok(None),
            _ => self.next_record(),
        }
    }

    /// Reads the `key:;value` lines.
    fn read_preamble(&mut self) -> Result<HashMap<String, String>, Box<dyn Error>> {
        let mut preamble = HashMap::new();
        while let Some(rec) = self.next_record_if(|rec| rec.len() == 2)? {
            preamble.insert(rec[0].to_string(), unwrap_value(&rec[1]));
        }
        Ok(preamble)
    }

    fn read_header(&mut self) -> Result<(), Box<dyn Error>> {
        match self.next_record()? {
            Some(rec) if is_header(&rec) => Ok(()),
            Some(rec) => Err(format!("invalid header: {rec:?}").into()),
            None => Err("unexpected end of file while looking for header".into()),
        }
    }

    fn read_lines(&mut self) -> Result<Vec<Line>, Box<dyn Error>> {
        let mut lines = Vec::new();
        while let Some(rec) = self.next_record_if(is_booking)? {
            lines.push(Line::parse(&rec)?);
        }
        Ok(lines)
    }

    /// Consumes the remaining disclaimer lines, which have a single field.
    fn read_trailer(&mut self) -> Result<(), Box<dyn Error>> {
        while let Some(rec) = self.next_record()? {
            if rec.len() != 1 {
                return Err(format!("invalid line: {rec:?}").into());
            }
        }
        Ok(())
    }
}

fn is_header(rec: &StringRecord) -> bool {
    is_booking(rec) && matches!(Field::Date.read(rec), "Datum" | "Buchungsdatum")
}

fn is_booking(rec: &StringRecord) -> bool {
    matches!(rec.len(), 7 | 8)
}

/// Some exports wrap preamble values Excel-style: `="CHF"`.
fn unwrap_value(s: &str) -> String {
    s.replace(['"', '='], "").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2025, 1, day).unwrap()
    }

    fn dec(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    fn line(day: u32, balance: Option<&str>) -> Line {
        Line {
            date: date(day),
            description: String::new(),
            quantity: Decimal::ZERO,
            balance: balance.map(dec),
        }
    }

    fn statement(lines: Vec<Line>) -> Statement {
        Statement {
            preamble: HashMap::new(),
            lines,
        }
    }

    #[test]
    fn test_parse_statement() {
        let source = "\
Konto:;=\"CH12 0900 0000 1234 5678 9\"
Währung:;=\"EUR\"
Datum;Avisierungstext;Gutschrift in EUR;Lastschrift in EUR;Label;Kategorie;Valuta;Saldo in EUR
03.01.2025;\"Kauf \"\"Migros\"\"\";;-42.15;;Lebensmittel;03.01.2025;1'000.00
02.01.2025;Lohn;1'042.15;;Lohn;;02.01.2025
Disclaimer:
\"Kein durch PostFinance erstelltes Dokument.\"
";
        let statement = Statement::parse(source).unwrap();
        assert_eq!(statement.currency(), "EUR");
        assert_eq!(
            statement.lines,
            vec![
                Line {
                    date: date(3),
                    description: "Kauf 'Migros' Lebensmittel".into(),
                    quantity: dec("-42.15"),
                    balance: Some(dec("1000.00")),
                },
                Line {
                    date: date(2),
                    description: "Lohn Lohn".into(),
                    quantity: dec("1042.15"),
                    balance: None,
                },
            ]
        );
        assert_eq!(statement.final_balance(), Some((date(3), dec("1000.00"))));
    }

    #[test]
    fn test_parse_errors() {
        let err = |source: &str| Statement::parse(source).unwrap_err().to_string();
        assert!(err("Währung:;CHF\n").starts_with("unexpected end of file"));
        assert!(err("Währung:;CHF\nfoo;bar;baz\n").starts_with("invalid header"));
        let header = "Datum;Avisierungstext;Gutschrift;Lastschrift;Label;Kategorie;Valuta;Saldo\n";
        assert!(err(&format!("{header}a;b;c\n")).starts_with("invalid line"));
        assert!(
            err(&format!("{header}01.01.2025;x;1;;;;01.01.2025\na;b;c\n"))
                .starts_with("invalid line")
        );
    }

    #[test]
    fn test_default_currency() {
        let source = "Datum;Avisierungstext;Gutschrift;Lastschrift;Label;Kategorie;Valuta;Saldo\n";
        assert_eq!(Statement::parse(source).unwrap().currency(), "CHF");
    }

    #[test]
    fn test_parse_quantity() {
        assert_eq!(parse_quantity("1'234.50", "").unwrap(), dec("1234.50"));
        assert_eq!(parse_quantity("", "-12.05").unwrap(), dec("-12.05"));
        assert!(parse_quantity("", "").is_err());
        assert!(parse_quantity("1", "-1").is_err());
    }

    #[test]
    fn test_final_balance() {
        assert_eq!(statement(vec![]).final_balance(), None);
        assert_eq!(statement(vec![line(1, None)]).final_balance(), None);
        // most recent first
        assert_eq!(
            statement(vec![
                line(3, Some("30")),
                line(3, Some("20")),
                line(1, Some("10"))
            ])
            .final_balance(),
            Some((date(3), dec("30")))
        );
        // oldest first
        assert_eq!(
            statement(vec![
                line(1, Some("10")),
                line(3, Some("20")),
                line(3, Some("30"))
            ])
            .final_balance(),
            Some((date(3), dec("30")))
        );
        // skips lines without balance
        assert_eq!(
            statement(vec![line(3, None), line(1, Some("10"))]).final_balance(),
            Some((date(1), dec("10")))
        );
    }

    #[test]
    fn test_decode_latin1() {
        assert_eq!(decode(b"W\xe4hrung"), "Währung");
        assert_eq!(decode("Währung".as_bytes()), "Währung");
    }
}
