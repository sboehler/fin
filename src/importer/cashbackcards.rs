//! Importer for Swisscard "Cashback Cards" credit card statements.
//!
//! Download the CSV export from the account management tool at
//! cashback-cards.ch.
//!
//! The export has one row per booking and twelve columns with a stable
//! header. `Betrag` is signed: charges are positive, payments and refunds
//! negative, so the card account is credited with the amount as written.

use std::{error::Error, io::Write, path::PathBuf, rc::Rc};

use chrono::NaiveDate;
use clap::Args;
use csv::StringRecord;
use rust_decimal::Decimal;

use crate::model::{
    entities::{Booking, Transaction},
    printer::Printer,
    registry::Registry,
};

/// The account which receives the counter-postings of all imported
/// transactions. The user is expected to replace it when reconciling
/// the imported journal.
const TBD_ACCOUNT: &str = "Expenses:TBD";

#[derive(Args)]
pub struct Command {
    source: PathBuf,

    /// The credit card account.
    #[arg(short, long)]
    account: String,
}

impl Command {
    pub fn run(&self, w: &mut impl Write) -> Result<(), Box<dyn Error>> {
        let source = std::fs::read_to_string(&self.source)?;
        import(&source, &self.account, w)
    }
}

/// Imports a Cashback Cards statement and writes the resulting journal to
/// `w`: one transaction per booking row, sorted by date.
pub fn import(source: &str, account: &str, w: &mut impl Write) -> Result<(), Box<dyn Error>> {
    let registry = Rc::new(Registry::new());
    let account = registry.account_id(account)?;
    let tbd = registry.account_id(TBD_ACCOUNT)?;

    let lines = parse(source)?;
    let mut transactions = lines
        .into_iter()
        .map(|line| {
            let currency = registry.commodity_id(&line.currency)?;
            Ok(Transaction {
                loc: None,
                date: line.date,
                description: Rc::new(line.description),
                // A charge is positive and reduces the card account, which
                // is the counterpart of the expense.
                bookings: Booking::create(account, tbd, line.quantity, currency, None),
                targets: None,
            })
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    transactions.sort_by_key(|t| t.date);

    Printer::new(w, registry).transactions(&transactions)?;
    Ok(())
}

fn parse(source: &str) -> Result<Vec<Line>, Box<dyn Error>> {
    let mut records = csv::ReaderBuilder::new()
        .has_headers(false)
        // Report a row with the wrong number of columns as an invalid line
        // rather than letting the reader complain about the header.
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(source.as_bytes())
        .into_records();
    match records.next().transpose()? {
        Some(rec) if Field::Date.read(&rec) == "Transaktionsdatum" => (),
        Some(rec) => return Err(format!("invalid header: {rec:?}").into()),
        None => return Err("unexpected end of file while looking for header".into()),
    }
    records
        .map(|rec| Line::parse(&rec?))
        .collect::<Result<Vec<_>, _>>()
}

/// A parsed booking row.
#[derive(Debug, PartialEq, Eq)]
struct Line {
    date: NaiveDate,
    description: String,
    /// Positive for charges, negative for payments and refunds.
    quantity: Decimal,
    currency: String,
}

/// Columns of a booking row, in file order.
#[derive(Clone, Copy)]
enum Field {
    Date,
    Description,
    Merchant,
    Card,
    Currency,
    Amount,
    ForeignCurrency,
    ForeignAmount,
    Direction,
    #[allow(dead_code)]
    Status,
    MerchantCategory,
    Category,
}

impl Field {
    fn read(self, rec: &StringRecord) -> &str {
        rec.get(self as usize).unwrap_or_default()
    }
}

impl Line {
    fn parse(rec: &StringRecord) -> Result<Line, Box<dyn Error>> {
        if rec.len() != 12 {
            return Err(format!("invalid line: {rec:?}").into());
        }
        let date = Field::Date.read(rec);
        let date = NaiveDate::parse_from_str(date, "%d.%m.%Y")
            .map_err(|e| format!("invalid date {date:?}: {e}"))?;
        let amount = Field::Amount.read(rec);
        let quantity = parse_decimal(amount)
            .map_err(|e| format!("invalid amount {amount:?} on {date}: {e}"))?;
        Ok(Line {
            date,
            description: describe(rec),
            quantity,
            currency: Field::Currency.read(rec).to_string(),
        })
    }
}

/// Joins the descriptive columns, dropping the ones this statement leaves
/// empty. The amount in the original currency is appended for transactions
/// which were converted, since the booked amount alone does not identify
/// them.
fn describe(rec: &StringRecord) -> String {
    let foreign = match (
        Field::ForeignAmount.read(rec),
        Field::ForeignCurrency.read(rec),
    ) {
        ("", _) | (_, "") => String::new(),
        (amount, currency) => format!("{amount} {currency}"),
    };
    [
        Field::Description.read(rec),
        Field::Merchant.read(rec),
        Field::MerchantCategory.read(rec),
        Field::Card.read(rec),
        Field::Category.read(rec),
        Field::Direction.read(rec),
        &foreign,
    ]
    .into_iter()
    .filter(|s| !s.is_empty())
    .collect::<Vec<_>>()
    .join(" / ")
    // Descriptions are printed as quoted strings, which cannot contain quotes.
    .replace('"', "'")
}

/// Parses an amount with optional thousands separators, e.g. `1'234.50`.
fn parse_decimal(s: &str) -> Result<Decimal, Box<dyn Error>> {
    Ok(s.replace('\'', "").parse()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const HEADER: &str = "Transaktionsdatum,Beschreibung,Händler,Kartennummer,Währung,Betrag,Fremdwährung,Betrag in Fremdwährung,Debit/Kredit,Status,Händlerkategorie,Registrierte Kategorie\n";

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 6, day).unwrap()
    }

    fn dec(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    #[test]
    fn test_parse() {
        let source = format!(
            "{HEADER}\
\"07.06.2026\",\"APPLE.COM/BILL\",\"Apple\",\"3400 00**** *0001\",\"CHF\",\"29.90\",\"\",\"\",\"Belastung\",\"Gebucht\",\"Dienstleistungen\",\"RECORD STORES\"\n\
\"12.05.2026\",\"IHRE ZAHLUNG\",\"\",\"3400 00**** *0001\",\"CHF\",\"-1'910.90\",\"\",\"\",\"Gutschrift\",\"Gebucht\",\"Zahlungen\",\"\"\n"
        );
        assert_eq!(
            parse(&source).unwrap(),
            vec![
                Line {
                    date: date(7),
                    description: "APPLE.COM/BILL / Apple / Dienstleistungen / 3400 00**** *0001 / RECORD STORES / Belastung".into(),
                    quantity: dec("29.90"),
                    currency: "CHF".into(),
                },
                Line {
                    // Empty Händler and Registrierte Kategorie are dropped.
                    date: NaiveDate::from_ymd_opt(2026, 5, 12).unwrap(),
                    description: "IHRE ZAHLUNG / Zahlungen / 3400 00**** *0001 / Gutschrift".into(),
                    quantity: dec("-1910.90"),
                    currency: "CHF".into(),
                },
            ]
        );
    }

    #[test]
    fn test_parse_foreign_currency() {
        let source = format!(
            "{HEADER}\
\"07.06.2026\",\"AMZN\",\"Amazon\",\"3400 00**** *0001\",\"CHF\",\"29.90\",\"USD\",\"32.15\",\"Belastung\",\"Gebucht\",\"Shopping\",\"\"\n"
        );
        let lines = parse(&source).unwrap();
        assert_eq!(
            lines[0].description,
            "AMZN / Amazon / Shopping / 3400 00**** *0001 / Belastung / 32.15 USD"
        );
    }

    #[test]
    fn test_parse_errors() {
        let err = |source: &str| parse(source).unwrap_err().to_string();
        assert!(err("").starts_with("unexpected end of file"));
        assert!(err("a,b,c\n").starts_with("invalid header"));
        assert!(err(&format!("{HEADER}a,b,c\n")).starts_with("invalid line"));
        let bad_date = format!("{HEADER}\"2026-06-07\",b,c,d,CHF,1.00,g,h,i,j,k,l\n");
        assert!(err(&bad_date).starts_with("invalid date"));
        let bad_amount = format!("{HEADER}\"07.06.2026\",b,c,d,CHF,x,g,h,i,j,k,l\n");
        assert!(err(&bad_amount).starts_with("invalid amount"));
    }

    /// A charge credits the card account; a payment debits it.
    #[test]
    fn test_import_direction() {
        let source = format!(
            "{HEADER}\
\"07.06.2026\",\"APPLE\",\"\",\"\",\"CHF\",\"29.90\",\"\",\"\",\"Belastung\",\"Gebucht\",\"\",\"\"\n\
\"08.06.2026\",\"ZAHLUNG\",\"\",\"\",\"CHF\",\"-100.00\",\"\",\"\",\"Gutschrift\",\"Gebucht\",\"\",\"\"\n"
        );
        let mut out = Vec::new();
        import(&source, "Liabilities:Card", &mut out).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "2026-06-07 \"APPLE / Belastung\"\n\
             Liabilities:Card Expenses:TBD          29.90 CHF\n\
             \n\
             2026-06-08 \"ZAHLUNG / Gutschrift\"\n\
             Expenses:TBD     Liabilities:Card     100.00 CHF\n"
        );
    }
}
