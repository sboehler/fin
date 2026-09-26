//! Importer for Revolut account statements.
//!
//! Revolut keeps one balance per currency and exports one CSV per currency:
//! in the app, open each account and download its statement in CSV format.
//! All of them are passed to a single invocation, which consolidates them
//! into one journal.
//!
//! The export is localized; English and German statements are recognized and
//! may be mixed. It has one row per booking and ten columns with a stable
//! header. `Amount` is signed and excludes `Fee`, which is charged on top of
//! it. Rows which never completed - reverted or still pending - carry neither
//! a completion date nor a balance and are skipped.
//!
//! A currency exchange moves money between two of those balances and is
//! therefore reported twice, once in each account's statement, both rows
//! sharing the time the exchange was started. The two legs are matched up
//! into a single transaction; a leg whose counterpart is missing, because the
//! other statement was not passed, is imported on its own.

use std::{
    collections::{HashMap, HashSet},
    error::Error,
    io::Write,
    path::PathBuf,
    rc::Rc,
};

use chrono::NaiveDate;
use clap::Args;
use csv::StringRecord;
use rust_decimal::Decimal;

use crate::model::{
    entities::{AccountID, Assertion, Booking, CommodityID, Transaction},
    printer::Printer,
    registry::Registry,
};

/// The account which receives the counter-postings of all imported
/// transactions. The user is expected to replace it when reconciling
/// the imported journal.
const TBD_ACCOUNT: &str = "Expenses:TBD";

#[derive(Args)]
pub struct Command {
    /// The statements to import, one per account currency.
    #[arg(required = true)]
    sources: Vec<PathBuf>,

    /// The Revolut account.
    #[arg(short, long)]
    account: String,

    /// The account fees are charged to.
    #[arg(short, long)]
    fee: String,

    /// The account the legs of a currency exchange are settled against.
    #[arg(short, long, default_value = TBD_ACCOUNT)]
    trading: String,
}

impl Command {
    pub fn run(&self, w: &mut impl Write) -> Result<(), Box<dyn Error>> {
        let sources = self
            .sources
            .iter()
            .map(|path| {
                let source = std::fs::read_to_string(path)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                Ok((path.display().to_string(), source))
            })
            .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
        let registry = Rc::new(Registry::new());
        let accounts = Accounts {
            account: registry.account_id(&self.account)?,
            fee: registry.account_id(&self.fee)?,
            trading: registry.account_id(&self.trading)?,
            tbd: registry.account_id(TBD_ACCOUNT)?,
        };
        let sources = sources
            .iter()
            .map(|(name, source)| (name.as_str(), source.as_str()))
            .collect::<Vec<_>>();
        import(&sources, registry, &accounts, w)
    }
}

/// The accounts the postings of an import are booked against.
struct Accounts {
    account: AccountID,
    fee: AccountID,
    trading: AccountID,
    tbd: AccountID,
}

/// Imports the given statements, each a `(name, contents)` pair where the
/// name only serves to report errors, and writes the resulting journal to
/// `w`: one transaction per booking row, with the two rows of a currency
/// exchange merged into one, sorted by date, followed by a balance assertion
/// per currency for the last balance its statement reports.
///
/// The assertions only hold if the statements reach back to the account's
/// first booking, or if the balances they start from are already in the
/// journal.
fn import(
    sources: &[(&str, &str)],
    registry: Rc<Registry>,
    accounts: &Accounts,
    w: &mut impl Write,
) -> Result<(), Box<dyn Error>> {
    let mut lines = Vec::new();
    for (name, source) in sources {
        lines.extend(parse(source).map_err(|e| format!("{name}: {e}"))?);
    }

    let builder = Builder {
        registry: registry.clone(),
        accounts,
    };
    let exchanges = match_exchanges(&lines);
    // The buy leg is printed as part of its sell leg's transaction.
    let matched = exchanges.values().copied().collect::<HashSet<_>>();
    let mut transactions = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if matched.contains(&i) {
            continue;
        }
        transactions.push(match exchanges.get(&i) {
            Some(&j) => builder.exchange(line, &lines[j])?,
            None => builder.booking(line)?,
        });
    }
    transactions.sort_by_key(|t| t.date);

    let assertions = builder.assertions(&lines)?;

    let mut printer = Printer::new(w, registry);
    printer.transactions(&transactions)?;
    if !assertions.is_empty() {
        printer.newline()?;
    }
    for assertion in &assertions {
        printer.assertion(assertion)?;
    }
    Ok(())
}

/// Matches the legs of the currency exchanges among `lines`, returning the
/// index of each buy leg by the index of the leg it was sold for.
fn match_exchanges(lines: &[Line]) -> HashMap<usize, usize> {
    let mut groups: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, line) in lines.iter().enumerate() {
        if line.exchange {
            groups.entry(&line.started).or_default().push(i);
        }
    }
    groups
        .into_values()
        .filter_map(|group| match group[..] {
            [i, j] => {
                let (a, b) = (&lines[i], &lines[j]);
                let opposite = a.quantity.is_sign_negative() != b.quantity.is_sign_negative();
                // Two exchanges of the same pair of currencies started in the
                // same second, or one leg of two unrelated exchanges, cannot
                // be told apart and are left for the reader to sort out.
                (opposite && a.currency != b.currency).then(|| {
                    if a.quantity.is_sign_negative() {
                        (i, j)
                    } else {
                        (j, i)
                    }
                })
            }
            _ => None,
        })
        .collect()
}

/// Turns statement rows into transactions.
struct Builder<'a> {
    registry: Rc<Registry>,
    accounts: &'a Accounts,
}

impl Builder<'_> {
    /// A single booking row, counter-booked against the TBD account.
    fn booking(&self, line: &Line) -> Result<Transaction, Box<dyn Error>> {
        let Accounts { account, tbd, .. } = *self.accounts;
        let currency = self.registry.commodity_id(&line.currency)?;
        let mut bookings = Booking::create(tbd, account, line.quantity, currency, None);
        bookings.extend(self.fee(line, currency));
        Ok(Transaction {
            loc: None,
            date: line.date,
            description: Rc::new(line.description.clone()),
            bookings,
            targets: None,
        })
    }

    /// Both legs of a currency exchange as one transaction: the account gives
    /// up one currency and receives the other, with the trading account
    /// absorbing the difference in value between the two.
    fn exchange(&self, sell: &Line, buy: &Line) -> Result<Transaction, Box<dyn Error>> {
        let Accounts {
            account, trading, ..
        } = *self.accounts;
        let sold = self.registry.commodity_id(&sell.currency)?;
        let bought = self.registry.commodity_id(&buy.currency)?;
        let mut bookings = Booking::create(trading, account, sell.quantity, sold, None);
        bookings.extend(Booking::create(
            trading,
            account,
            buy.quantity,
            bought,
            None,
        ));
        bookings.extend(self.fee(sell, sold));
        bookings.extend(self.fee(buy, bought));
        Ok(Transaction {
            loc: None,
            // Both legs complete at the same time and describe the exchange
            // by the currency bought.
            date: sell.date,
            description: Rc::new(buy.description.clone()),
            bookings,
            targets: None,
        })
    }

    /// The fee charged on a row, if any. It is deducted from the account on
    /// top of the row's amount and in the same currency.
    fn fee(&self, line: &Line, currency: CommodityID) -> Vec<Booking> {
        let Accounts { account, fee, .. } = *self.accounts;
        if line.fee.is_zero() {
            return Vec::new();
        }
        Booking::create(account, fee, line.fee, currency, None)
    }

    /// One assertion per currency, for the balance after the last booking
    /// reported in it.
    fn assertions(&self, lines: &[Line]) -> Result<Vec<Assertion>, Box<dyn Error>> {
        let mut last: HashMap<&str, &Line> = HashMap::new();
        for line in lines {
            last.entry(&line.currency)
                .and_modify(|l| {
                    if line.date >= l.date {
                        *l = line
                    }
                })
                .or_insert(line);
        }
        let mut assertions = last
            .into_values()
            .map(|line| {
                Ok(Assertion {
                    loc: None,
                    date: line.date,
                    account: self.accounts.account,
                    balance: line.balance,
                    commodity: self.registry.commodity_id(&line.currency)?,
                })
            })
            .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
        assertions.sort_by_key(|a| (a.date, self.registry.commodity_name(a.commodity)));
        Ok(assertions)
    }
}

/// The language a statement is exported in. Revolut localizes the column
/// names and the booking types, but not the dates or the numbers.
struct Language {
    header: [&'static str; 10],
    /// The booking type of one leg of a currency exchange.
    exchange: &'static str,
}

const LANGUAGES: [Language; 2] = [
    Language {
        header: [
            "Type",
            "Product",
            "Started Date",
            "Completed Date",
            "Description",
            "Amount",
            "Fee",
            "Currency",
            "State",
            "Balance",
        ],
        exchange: "Exchange",
    },
    Language {
        header: [
            "Art",
            "Produkt",
            "Datum des Beginns",
            "Datum des Abschlusses",
            "Beschreibung",
            "Betrag",
            "Gebühr",
            "Währung",
            "Status",
            "Kontostand",
        ],
        exchange: "Umtausch",
    },
];

fn parse(source: &str) -> Result<Vec<Line>, Box<dyn Error>> {
    let mut records = csv::ReaderBuilder::new()
        .has_headers(false)
        // Report a row with the wrong number of columns as an invalid line
        // rather than letting the reader complain about the header.
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(source.as_bytes())
        .into_records();
    let header = match records.next().transpose()? {
        Some(header) => header,
        None => return Err("unexpected end of file while looking for header".into()),
    };
    let language = LANGUAGES
        .iter()
        .find(|language| header.iter().eq(language.header))
        .ok_or_else(|| format!("invalid header: {header:?}"))?;
    let mut lines = Vec::new();
    for record in records {
        if let Some(line) = Line::parse(&record?, language)? {
            lines.push(line);
        }
    }
    Ok(lines)
}

/// A parsed booking row.
#[derive(Debug, PartialEq, Eq)]
struct Line {
    /// The date the booking completed.
    date: NaiveDate,
    /// The time the booking was started, as written. Both legs of a currency
    /// exchange carry the same one, which is what matches them up.
    started: String,
    /// Whether the row is one leg of a currency exchange.
    exchange: bool,
    description: String,
    /// Positive for credits, negative for debits.
    quantity: Decimal,
    /// Charged on top of `quantity`, positive.
    fee: Decimal,
    currency: String,
    /// The balance of the account in `currency` after this booking.
    balance: Decimal,
}

/// Columns of a booking row, in file order.
#[derive(Clone, Copy)]
enum Field {
    Type,
    #[allow(dead_code)]
    Product,
    StartedDate,
    CompletedDate,
    Description,
    Amount,
    Fee,
    Currency,
    #[allow(dead_code)]
    State,
    Balance,
}

impl Field {
    fn read(self, rec: &StringRecord) -> &str {
        rec.get(self as usize).unwrap_or_default()
    }
}

impl Line {
    /// Parses a booking row, or returns `None` if it never completed.
    fn parse(rec: &StringRecord, language: &Language) -> Result<Option<Line>, Box<dyn Error>> {
        if rec.len() != 10 {
            return Err(format!("invalid line: {rec:?}").into());
        }
        // A reverted or pending row has no completion date and no balance,
        // and did not move any money.
        let completed = Field::CompletedDate.read(rec);
        if completed.is_empty() {
            return Ok(None);
        }
        // The column holds a timestamp, of which only the day is booked.
        let date = completed.get(..10).unwrap_or(completed);
        let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .map_err(|e| format!("invalid completed date {completed:?}: {e}"))?;
        let amount = Field::Amount.read(rec);
        let quantity = amount
            .parse()
            .map_err(|e| format!("invalid amount {amount:?} on {date}: {e}"))?;
        let fee = Field::Fee.read(rec);
        let fee = fee
            .parse()
            .map_err(|e| format!("invalid fee {fee:?} on {date}: {e}"))?;
        let balance = Field::Balance.read(rec);
        let balance = balance
            .parse()
            .map_err(|e| format!("invalid balance {balance:?} on {date}: {e}"))?;
        Ok(Some(Line {
            date,
            started: Field::StartedDate.read(rec).to_string(),
            exchange: Field::Type.read(rec) == language.exchange,
            // Descriptions are printed as quoted strings, which cannot
            // contain quotes.
            description: Field::Description.read(rec).replace('"', "'"),
            quantity,
            fee,
            currency: Field::Currency.read(rec).to_string(),
            balance,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const DE_HEADER: &str = "Art,Produkt,Datum des Beginns,Datum des Abschlusses,Beschreibung,Betrag,Gebühr,Währung,Status,Kontostand\n";
    const EN_HEADER: &str =
        "Type,Product,Started Date,Completed Date,Description,Amount,Fee,Currency,State,Balance\n";

    fn date(month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, month, day).unwrap()
    }

    fn dec(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    fn import_to_string(sources: &[(&str, &str)]) -> String {
        let registry = Rc::new(Registry::new());
        let accounts = Accounts {
            account: registry.account_id("Assets:Revolut").unwrap(),
            fee: registry.account_id("Expenses:Fees").unwrap(),
            trading: registry.account_id("Expenses:Trading").unwrap(),
            tbd: registry.account_id(TBD_ACCOUNT).unwrap(),
        };
        let mut out = Vec::new();
        import(sources, registry, &accounts, &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn test_parse() {
        let source = format!(
            "{DE_HEADER}\
Kartenbezahlung,Giro,2026-01-10 14:23:45,2026-01-11 03:50:34,Backblaze,-12.52,0.13,CHF,ABGESCHLOSSEN,935.82\n\
Kartenbezahlung,Giro,2026-02-13 22:05:15,,Etsy,-1.00,0.00,CHF,STORNIERT,\n\
Umtausch,Giro,2026-01-19 20:40:40,2026-01-19 20:40:40,Umgetauscht in EUR,-200.00,0.00,CHF,ABGESCHLOSSEN,732.73\n"
        );
        assert_eq!(
            parse(&source).unwrap(),
            vec![
                Line {
                    // The row is booked on the day it completed, not the one
                    // it was started on.
                    date: date(1, 11),
                    started: "2026-01-10 14:23:45".into(),
                    exchange: false,
                    description: "Backblaze".into(),
                    quantity: dec("-12.52"),
                    fee: dec("0.13"),
                    currency: "CHF".into(),
                    balance: dec("935.82"),
                },
                // The reverted row is dropped.
                Line {
                    date: date(1, 19),
                    started: "2026-01-19 20:40:40".into(),
                    exchange: true,
                    description: "Umgetauscht in EUR".into(),
                    quantity: dec("-200.00"),
                    fee: dec("0.00"),
                    currency: "CHF".into(),
                    balance: dec("732.73"),
                },
            ]
        );
    }

    /// The booking type is localized along with the header, so an exchange is
    /// only recognized by the word the statement's own language uses.
    #[test]
    fn test_parse_recognizes_exchange_per_language() {
        let exchange = |header: &str, kind: &str| {
            let source = format!(
                "{header}{kind},Current,2026-01-19 20:40:40,2026-01-19 20:40:40,d,1.00,0.00,EUR,COMPLETED,1.00\n"
            );
            parse(&source).unwrap()[0].exchange
        };
        assert!(exchange(EN_HEADER, "Exchange"));
        assert!(exchange(DE_HEADER, "Umtausch"));
        assert!(!exchange(EN_HEADER, "Umtausch"));
        assert!(!exchange(DE_HEADER, "Exchange"));
    }

    #[test]
    fn test_parse_errors() {
        let err = |source: &str| parse(source).unwrap_err().to_string();
        assert!(err("").starts_with("unexpected end of file"));
        assert!(err("a,b,c\n").starts_with("invalid header"));
        assert!(err(&format!("{EN_HEADER}a,b,c\n")).starts_with("invalid line"));
        let row = |amount, fee, balance| {
            format!(
                "Card payment,Current,2026-01-10 14:23:45,2026-01-11 03:50:34,d,{amount},{fee},CHF,COMPLETED,{balance}\n"
            )
        };
        assert!(
            err(&format!("{EN_HEADER}{}", row("x", "0.00", "1.00"))).starts_with("invalid amount")
        );
        assert!(
            err(&format!("{EN_HEADER}{}", row("1.00", "x", "1.00"))).starts_with("invalid fee")
        );
        assert!(
            err(&format!("{EN_HEADER}{}", row("1.00", "0.00", "x"))).starts_with("invalid balance")
        );
        let bad_date = format!(
            "{EN_HEADER}Card payment,Current,2026-01-10 14:23:45,11.01.2026,d,1.00,0.00,CHF,COMPLETED,1.00\n"
        );
        assert!(err(&bad_date).starts_with("invalid completed date"));
    }

    /// Errors name the statement they came from, which is the only way to
    /// tell three files of the same export apart.
    #[test]
    fn test_import_error_names_the_source() {
        let registry = Rc::new(Registry::new());
        let accounts = Accounts {
            account: registry.account_id("Assets:Revolut").unwrap(),
            fee: registry.account_id("Expenses:Fees").unwrap(),
            trading: registry.account_id("Expenses:Trading").unwrap(),
            tbd: registry.account_id(TBD_ACCOUNT).unwrap(),
        };
        let err = import(
            &[(EN_HEADER, EN_HEADER), ("broken.csv", "a,b,c\n")],
            registry,
            &accounts,
            &mut Vec::new(),
        )
        .unwrap_err()
        .to_string();
        assert!(err.starts_with("broken.csv: invalid header"), "{err}");
    }

    /// An amount is booked against the TBD account, a fee against the fee
    /// account, and the statement's last balance is asserted.
    #[test]
    fn test_import_booking_and_fee() {
        let source = format!(
            "{EN_HEADER}\
Card payment,Current,2026-01-10 14:23:45,2026-01-11 03:50:34,Backblaze,-12.52,0.13,CHF,COMPLETED,935.82\n\
Topup,Current,2026-01-12 14:23:45,2026-01-12 14:23:45,Payment from M.,100.00,0.00,CHF,COMPLETED,1035.82\n"
        );
        assert_eq!(
            import_to_string(&[("chf.csv", &source)]),
            "2026-01-11\n\
             \x20 Backblaze\n\
             Assets:Revolut\n\
             -> Expenses:Fees        0.13 CHF\n\
             -> Expenses:TBD        12.52 CHF\n\
             \n\
             2026-01-12\n\
             \x20 Payment from M.\n\
             Assets:Revolut\n\
             <- Expenses:TBD       100.00 CHF\n\
             \n\
             2026-01-12 balance Assets:Revolut 1035.82 CHF\n"
        );
    }

    /// The two statements report the exchange once each; it is imported once.
    #[test]
    fn test_import_matches_exchange() {
        let chf = format!(
            "{DE_HEADER}\
Umtausch,Giro,2026-08-07 09:49:54,2026-08-07 09:49:54,Umgetauscht in EUR,-500.00,0.00,CHF,ABGESCHLOSSEN,1896.03\n"
        );
        let eur = format!(
            "{DE_HEADER}\
Umtausch,Giro,2026-08-07 09:49:54,2026-08-07 09:49:54,Umgetauscht in EUR,533.54,0.00,EUR,ABGESCHLOSSEN,559.43\n"
        );
        let expected = "2026-08-07\n\
                        \x20 Umgetauscht in EUR\n\
                        Assets:Revolut\n\
                        <- Expenses:Trading     533.54 EUR\n\
                        -> Expenses:Trading     500.00 CHF\n\
                        \n\
                        2026-08-07 balance Assets:Revolut 1896.03 CHF\n\
                        2026-08-07 balance Assets:Revolut 559.43 EUR\n";
        assert_eq!(
            import_to_string(&[("chf.csv", &chf), ("eur.csv", &eur)]),
            expected
        );
        // Which statement is passed first does not matter.
        assert_eq!(
            import_to_string(&[("eur.csv", &eur), ("chf.csv", &chf)]),
            expected
        );
    }

    /// Without the statement holding the other leg there is nothing to match,
    /// and the leg is booked on its own.
    #[test]
    fn test_import_unmatched_exchange() {
        let chf = format!(
            "{DE_HEADER}\
Umtausch,Giro,2026-08-07 09:49:54,2026-08-07 09:49:54,Umgetauscht in EUR,-500.00,0.00,CHF,ABGESCHLOSSEN,1896.03\n"
        );
        assert_eq!(
            import_to_string(&[("chf.csv", &chf)]),
            "2026-08-07\n\
             \x20 Umgetauscht in EUR\n\
             Assets:Revolut\n\
             -> Expenses:TBD       500.00 CHF\n\
             \n\
             2026-08-07 balance Assets:Revolut 1896.03 CHF\n"
        );
    }

    /// Legs which do not pair up one to one are left alone rather than
    /// guessed at: two exchanges started in the same second, and a leg whose
    /// counterpart is in the same currency.
    #[test]
    fn test_import_ambiguous_exchanges() {
        let row = |amount: &str, currency: &str, balance: &str| {
            format!(
                "Exchange,Current,2026-08-07 09:49:54,2026-08-07 09:49:54,Exchanged to X,{amount},0.00,{currency},COMPLETED,{balance}\n"
            )
        };
        let three = format!(
            "{EN_HEADER}{}{}{}",
            row("-500.00", "CHF", "1.00"),
            row("533.54", "EUR", "2.00"),
            row("100.00", "USD", "3.00")
        );
        assert_eq!(
            import_to_string(&[("a.csv", &three)])
                .matches("Exchanged to X")
                .count(),
            3
        );
        let same_currency = format!(
            "{EN_HEADER}{}{}",
            row("-500.00", "CHF", "1.00"),
            row("500.00", "CHF", "2.00")
        );
        assert_eq!(
            import_to_string(&[("a.csv", &same_currency)])
                .matches("Exchanged to X")
                .count(),
            2
        );
    }

    /// One assertion per currency, for the last balance reported in it, even
    /// when a currency is spread over several statements.
    #[test]
    fn test_import_asserts_last_balance_per_currency() {
        let row = |day: u32, currency: &str, balance: &str| {
            format!(
                "Card payment,Current,2026-01-{day:02} 14:23:45,2026-01-{day:02} 15:23:45,d,-1.00,0.00,{currency},COMPLETED,{balance}\n"
            )
        };
        let first = format!(
            "{EN_HEADER}{}{}",
            row(10, "CHF", "10.00"),
            row(11, "CHF", "9.00")
        );
        let second = format!("{EN_HEADER}{}", row(12, "CHF", "8.00"));
        let eur = format!("{EN_HEADER}{}", row(9, "EUR", "7.00"));
        let journal = import_to_string(&[("a.csv", &first), ("b.csv", &second), ("c.csv", &eur)]);
        let assertions = journal
            .lines()
            .filter(|l| l.contains("balance"))
            .collect::<Vec<_>>();
        assert_eq!(
            assertions,
            vec![
                "2026-01-09 balance Assets:Revolut 7.00 EUR",
                "2026-01-12 balance Assets:Revolut 8.00 CHF",
            ]
        );
    }
}
