//! Importer for Swissquote accounts.
//!
//! In the web interface, open the transactions overview, pick the date range
//! and export it as CSV. The file is Latin-1 encoded and semicolon
//! separated, has thirteen columns with a stable German header, and lists
//! the newest row first.
//!
//! `Nettobetrag` is the signed cash amount which moved and `Kosten` the fee
//! or the withholding tax already deducted from it, so a trade books the
//! gross amount against the trading account and the fee separately, and a
//! dividend the gross payment against the dividend account and the tax
//! against the tax account.
//!
//! A currency exchange is reported as two rows, a debit and a credit sharing
//! a timestamp, which are matched up into a single transaction.
//!
//! Swissquote keeps one balance per currency: `Saldo` is the balance in the
//! row's own currency, and one assertion per currency is emitted for the
//! last one reported.
//!
//! The `Aufgelaufene Zinsen` column, which only bond trades carry, is part
//! of `Nettobetrag` and is not booked separately.

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    error::Error,
    io::Write,
    path::PathBuf,
    rc::Rc,
};

use chrono::{NaiveDate, NaiveDateTime};
use clap::Args;
use csv::StringRecord;
use rust_decimal::Decimal;

use crate::model::{
    entities::{AccountID, Assertion, Booking, CommodityID, Transaction},
    printer::Printer,
    registry::Registry,
};

/// The account which receives the counter-postings of cash transfers and of
/// rows this importer does not know. The user is expected to replace it when
/// reconciling the imported journal.
const TBD_ACCOUNT: &str = "Expenses:TBD";

#[derive(Args)]
pub struct Command {
    source: PathBuf,

    /// The Swissquote account.
    #[arg(short, long)]
    account: String,

    /// The dividend income account.
    #[arg(short, long)]
    dividend: String,

    /// The interest income account.
    #[arg(short, long)]
    interest: String,

    /// The withholding tax account.
    #[arg(short = 'w', long)]
    tax: String,

    /// The fee account.
    #[arg(short, long)]
    fee: String,

    /// The trading gain / loss account.
    #[arg(short, long)]
    trading: String,
}

impl Command {
    pub fn run(&self, w: &mut impl Write) -> Result<(), Box<dyn Error>> {
        let source = std::fs::read(&self.source)?;
        let registry = Rc::new(Registry::new());
        let accounts = Accounts {
            account: registry.account_id(&self.account)?,
            dividend: registry.account_id(&self.dividend)?,
            interest: registry.account_id(&self.interest)?,
            tax: registry.account_id(&self.tax)?,
            fee: registry.account_id(&self.fee)?,
            trading: registry.account_id(&self.trading)?,
            tbd: registry.account_id(TBD_ACCOUNT)?,
        };
        import(&source, registry, &accounts, w)
    }
}

/// The accounts the postings of an import are booked against.
struct Accounts {
    account: AccountID,
    dividend: AccountID,
    interest: AccountID,
    tax: AccountID,
    fee: AccountID,
    trading: AccountID,
    tbd: AccountID,
}

/// Imports a Swissquote transactions export and writes the resulting journal
/// to `w`: one transaction per row, with the two legs of a currency exchange
/// merged into one, ordered by the time the rows were booked, followed by a
/// balance assertion per currency for the last balance reported in it.
///
/// The assertions only hold if the export reaches back to the account's
/// first booking, or if the balances it starts from are already in the
/// journal.
fn import(
    source: &[u8],
    registry: Rc<Registry>,
    accounts: &Accounts,
    w: &mut impl Write,
) -> Result<(), Box<dyn Error>> {
    let lines = parse(&decode(source))?;
    let builder = Builder {
        registry: registry.clone(),
        accounts,
    };

    let exchanges = match_exchanges(&lines);
    // The second leg is printed as part of the first leg's transaction.
    let matched = exchanges.values().copied().collect::<HashSet<_>>();
    let mut transactions = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if matched.contains(&i) {
            continue;
        }
        let transaction = match exchanges.get(&i) {
            Some(&j) => builder.exchange(line, &lines[j])?,
            None => match line.kind {
                Kind::Buy | Kind::Sell => builder.trade(line)?,
                Kind::Dividend => builder.dividend(line)?,
                // Custody fees are charged on the portfolio as a whole, so
                // they are performance-relevant without naming a commodity.
                Kind::Fee => builder.cash(line, accounts.fee, Some(Vec::new()))?,
                Kind::Interest => {
                    let currency = builder.commodity(&line.currency)?;
                    builder.cash(line, accounts.interest, Some(vec![currency]))?
                }
                // A leg whose counterpart is missing still exchanged one
                // currency for another, so it is settled against the trading
                // account rather than guessed at.
                Kind::Forex => builder.cash(line, accounts.trading, None)?,
                Kind::Transfer | Kind::Other => builder.cash(line, accounts.tbd, None)?,
            },
        };
        transactions.push((line.time, transaction));
    }
    // The export lists the newest row first; the timestamp orders the rows
    // within a day as well as across days.
    transactions.sort_by_key(|(time, _)| *time);
    let transactions = transactions
        .into_iter()
        .map(|(_, transaction)| transaction)
        .collect::<Vec<_>>();

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

/// Exports are Latin-1 encoded; a file which is valid UTF-8 is taken as
/// such, so that an export which ever changes encoding is still read.
fn decode(bytes: &[u8]) -> Cow<'_, str> {
    match std::str::from_utf8(bytes) {
        Ok(s) => Cow::Borrowed(s),
        Err(_) => Cow::Owned(bytes.iter().map(|&b| b as char).collect()),
    }
}

/// Matches the legs of the currency exchanges among `lines`, returning the
/// index of the second leg by the index of the first.
fn match_exchanges(lines: &[Line]) -> HashMap<usize, usize> {
    let mut groups: HashMap<NaiveDateTime, Vec<usize>> = HashMap::new();
    for (i, line) in lines.iter().enumerate() {
        if line.kind == Kind::Forex {
            groups.entry(line.time).or_default().push(i);
        }
    }
    groups
        .into_values()
        .filter_map(|group| match group[..] {
            [i, j] => {
                let (a, b) = (&lines[i], &lines[j]);
                let opposite = a.amount.is_sign_negative() != b.amount.is_sign_negative();
                // Two exchanges booked in the same second, or one leg of two
                // unrelated exchanges, cannot be told apart and are left for
                // the reader to sort out.
                (opposite && a.currency != b.currency).then_some((i, j))
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
    /// A purchase or a sale: the security moves in one direction and the
    /// cash in the other, both against the trading account, with the fee
    /// booked on top of the net amount the export reports.
    fn trade(&self, line: &Line) -> Result<Transaction, Box<dyn Error>> {
        let Accounts {
            account,
            trading,
            fee,
            ..
        } = *self.accounts;
        if line.symbol.is_empty() {
            return Err(format!("{} on {} without a symbol", line.kind_name, line.date).into());
        }
        let asset = self.commodity(&line.symbol)?;
        let currency = self.commodity(&line.currency)?;
        let quantity = match line.kind {
            Kind::Sell => -line.quantity,
            _ => line.quantity,
        };
        // The net amount is reported after the fee, while the journal books
        // the gross trade against the trading account and the fee apart.
        let proceeds = line.amount + line.fee;
        let mut bookings = Booking::create(trading, account, quantity.normalize(), asset, None);
        bookings.extend(Booking::create(trading, account, proceeds, currency, None));
        if !line.fee.is_zero() {
            bookings.extend(Booking::create(fee, account, -line.fee, currency, None));
        }
        Ok(Transaction {
            loc: None,
            date: line.date,
            description: Rc::new(line.describe_trade()),
            bookings,
            targets: Some(vec![asset, currency]),
        })
    }

    /// A dividend, a capital gain or a repayment of capital, attributed to
    /// the position it was paid on. `Kosten` holds the withholding tax
    /// deducted from the payment.
    fn dividend(&self, line: &Line) -> Result<Transaction, Box<dyn Error>> {
        let Accounts {
            account,
            dividend,
            tax,
            ..
        } = *self.accounts;
        let currency = self.commodity(&line.currency)?;
        let gross = line.amount + line.fee;
        let mut bookings = Booking::create(dividend, account, gross, currency, None);
        if !line.fee.is_zero() {
            bookings.extend(Booking::create(account, tax, line.fee, currency, None));
        }
        // A payment which does not name a position, e.g. a capital gain
        // distributed in cash, is attributed to none.
        let targets = match line.symbol.is_empty() {
            true => None,
            false => Some(vec![self.commodity(&line.symbol)?]),
        };
        Ok(Transaction {
            loc: None,
            date: line.date,
            description: Rc::new(line.describe()),
            bookings,
            targets,
        })
    }

    /// Both legs of a currency exchange as one transaction: the account
    /// gives up one currency and receives the other, with the trading
    /// account absorbing the difference in value between the two.
    fn exchange(&self, first: &Line, second: &Line) -> Result<Transaction, Box<dyn Error>> {
        let Accounts {
            account, trading, ..
        } = *self.accounts;
        let (a, b) = (
            self.commodity(&first.currency)?,
            self.commodity(&second.currency)?,
        );
        let mut bookings = Booking::create(trading, account, first.amount, a, None);
        bookings.extend(Booking::create(trading, account, second.amount, b, None));
        Ok(Transaction {
            loc: None,
            date: first.date,
            description: Rc::new(format!(
                "{} {} {} / {} {} {}",
                first.kind_name,
                first.amount,
                first.currency,
                second.kind_name,
                second.amount,
                second.currency
            )),
            bookings,
            targets: Some(vec![a, b]),
        })
    }

    /// A row which only moves cash, booked against `credit`. The net amount
    /// is signed, so a fee reduces the account and a deposit increases it.
    fn cash(
        &self,
        line: &Line,
        credit: AccountID,
        targets: Option<Vec<CommodityID>>,
    ) -> Result<Transaction, Box<dyn Error>> {
        let currency = self.commodity(&line.currency)?;
        Ok(Transaction {
            loc: None,
            date: line.date,
            description: Rc::new(line.describe()),
            bookings: Booking::create(credit, self.accounts.account, line.amount, currency, None),
            targets,
        })
    }

    /// One assertion per currency, for the balance after the last row
    /// reported in it.
    fn assertions(&self, lines: &[Line]) -> Result<Vec<Assertion>, Box<dyn Error>> {
        let mut last: HashMap<&str, &Line> = HashMap::new();
        for line in lines {
            last.entry(&line.currency)
                .and_modify(|l| {
                    if line.time >= l.time {
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
                    commodity: self.commodity(&line.currency)?,
                })
            })
            .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
        assertions.sort_by_key(|a| (a.date, self.registry.commodity_name(a.commodity)));
        Ok(assertions)
    }

    fn commodity(&self, name: &str) -> Result<CommodityID, Box<dyn Error>> {
        Ok(self.registry.commodity_id(name)?)
    }
}

/// What a row does, as classified by its `Transaktionen` column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Buy,
    Sell,
    /// One leg of a currency exchange.
    Forex,
    /// A dividend, a capital gain or a repayment of capital.
    Dividend,
    /// Custody fees and the VAT charged on them.
    Fee,
    /// Cash moved to or from the outside world.
    Transfer,
    Interest,
    /// Anything else, booked against the TBD account.
    Other,
}

impl Kind {
    fn parse(s: &str) -> Kind {
        match s {
            "Kauf" => Kind::Buy,
            "Verkauf" => Kind::Sell,
            "Forex-Gutschrift"
            | "Forex-Belastung"
            | "Fx-Gutschrift Comp."
            | "Fx-Belastung Comp." => Kind::Forex,
            "Dividende" | "Capital Gain" | "Kapitalrückzahlung" => Kind::Dividend,
            "Depotgebühren" => Kind::Fee,
            "Einzahlung" | "Auszahlung" | "Vergütung" | "Belastung" => Kind::Transfer,
            "Zins" => Kind::Interest,
            _ => Kind::Other,
        }
    }
}

/// A parsed row.
#[derive(Debug, PartialEq, Eq)]
struct Line {
    /// The day the row is booked on.
    date: NaiveDate,
    /// The full timestamp, which orders the rows and matches up the legs of
    /// a currency exchange.
    time: NaiveDateTime,
    /// The order number, `00000000` for a row which is not a trade.
    order: String,
    kind: Kind,
    /// The `Transaktionen` column as written, which names the row in its
    /// description.
    kind_name: String,
    symbol: String,
    name: String,
    isin: String,
    /// The number of securities traded; one for a row which is not a trade.
    quantity: Decimal,
    /// The price per security, or the gross amount of a payment.
    price: Decimal,
    /// The fee charged on the row, or the withholding tax deducted from it.
    /// Positive, and already included in `amount`.
    fee: Decimal,
    /// The cash which moved, positive for credits and negative for debits.
    amount: Decimal,
    /// The balance of the account in `currency` after this row.
    balance: Decimal,
    currency: String,
}

impl Line {
    fn parse(rec: &StringRecord) -> Result<Line, Box<dyn Error>> {
        if rec.len() != HEADER.len() {
            return Err(format!("invalid line: {rec:?}").into());
        }
        let time = Field::Date.read(rec);
        let time = NaiveDateTime::parse_from_str(time, "%d-%m-%Y %H:%M:%S")
            .map_err(|e| format!("invalid date {time:?}: {e}"))?;
        let date = time.date();
        let decimal = |field: Field, what: &str| -> Result<Decimal, Box<dyn Error>> {
            let s = field.read(rec);
            Ok(parse_decimal(s).map_err(|e| format!("invalid {what} {s:?} on {date}: {e}"))?)
        };
        let kind_name = Field::Type.read(rec);
        Ok(Line {
            date,
            time,
            order: Field::Order.read(rec).to_string(),
            kind: Kind::parse(kind_name),
            kind_name: kind_name.to_string(),
            symbol: Field::Symbol.read(rec).to_string(),
            name: Field::Name.read(rec).to_string(),
            isin: Field::Isin.read(rec).to_string(),
            quantity: decimal(Field::Quantity, "quantity")?,
            price: decimal(Field::Price, "price")?,
            fee: decimal(Field::Fee, "fee")?,
            amount: decimal(Field::Amount, "amount")?,
            balance: decimal(Field::Balance, "balance")?,
            currency: Field::Currency.read(rec).to_string(),
        })
    }

    /// `<order> <kind> <quantity> x <security> @ <price> <currency>`, the
    /// terms of the trade as the export reports them.
    fn describe_trade(&self) -> String {
        let security = join(&[&self.symbol, &self.name, &self.isin]);
        format!(
            "{order} {kind} {quantity} x {security} @ {price} {currency}",
            order = self.order,
            kind = self.kind_name,
            quantity = self.quantity.normalize(),
            price = self.price.normalize(),
            currency = self.currency,
        )
    }

    /// The row's type and, where it names one, the security it is about.
    fn describe(&self) -> String {
        join(&[&self.kind_name, &self.symbol, &self.name, &self.isin])
    }
}

/// Joins the non-empty parts of a description with spaces. Descriptions are
/// printed as quoted strings, which cannot contain quotes.
fn join(parts: &[&str]) -> String {
    parts
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('"', "'")
}

/// Columns of a row, in file order.
#[derive(Clone, Copy)]
enum Field {
    Date,
    Order,
    Type,
    Symbol,
    Name,
    Isin,
    Quantity,
    Price,
    Fee,
    /// Accrued interest, which only bond trades carry. It is part of the net
    /// amount and is not booked separately.
    #[allow(dead_code)]
    AccruedInterest,
    Amount,
    Balance,
    Currency,
}

impl Field {
    fn read(self, rec: &StringRecord) -> &str {
        rec.get(self as usize).unwrap_or_default()
    }
}

const HEADER: [&str; 13] = [
    "Datum",
    "Auftrag #",
    "Transaktionen",
    "Symbol",
    "Name",
    "ISIN",
    "Anzahl",
    "Stückpreis",
    "Kosten",
    "Aufgelaufene Zinsen",
    "Nettobetrag",
    "Saldo",
    "Währung",
];

fn parse(source: &str) -> Result<Vec<Line>, Box<dyn Error>> {
    let mut records = csv::ReaderBuilder::new()
        .has_headers(false)
        // Report a row with the wrong number of columns as an invalid line
        // rather than letting the reader complain about the header.
        .flexible(true)
        .delimiter(b';')
        .trim(csv::Trim::All)
        .from_reader(source.as_bytes())
        .into_records();
    match records.next().transpose()? {
        Some(rec) if rec.iter().eq(HEADER) => (),
        Some(rec) => return Err(format!("invalid header: {rec:?}").into()),
        None => return Err("unexpected end of file while looking for header".into()),
    }
    records
        .map(|rec| Line::parse(&rec?))
        .collect::<Result<Vec<_>, _>>()
}

/// Parses an amount with optional thousands separators, e.g. `1'234.50`.
fn parse_decimal(s: &str) -> Result<Decimal, rust_decimal::Error> {
    s.replace('\'', "").parse()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// A row of the export, by column in file order.
    type Row = [&'static str; 13];

    const BUY: Row = [
        "25-03-2026 09:47:33",
        "288382377",
        "Kauf",
        "VWRL",
        "Vanguard All-World",
        "IE00B3RBWM25",
        "13.0",
        "128.98",
        "14.37",
        "0.00",
        "-1'691.12",
        "123.21",
        "CHF",
    ];
    const CUSTODY_FEE: Row = [
        "30-06-2026 12:58:05",
        "00000000",
        "Depotgebühren",
        "",
        "",
        "",
        "1.0",
        "50.00",
        "4.05",
        "0.00",
        "-54.05",
        "15.11",
        "CHF",
    ];

    fn source(rows: &[Row]) -> String {
        let mut source = format!("{}\n", HEADER.join(";"));
        for row in rows {
            source.push_str(&format!("{}\n", row.join(";")));
        }
        source
    }

    fn date(month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, month, day).unwrap()
    }

    fn dec(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    fn import_to_string(rows: &[Row]) -> String {
        let registry = Rc::new(Registry::new());
        let accounts = Accounts {
            account: registry.account_id("Assets:Swissquote").unwrap(),
            dividend: registry.account_id("Income:Dividends").unwrap(),
            interest: registry.account_id("Income:Interest").unwrap(),
            tax: registry.account_id("Expenses:Tax").unwrap(),
            fee: registry.account_id("Expenses:Fees").unwrap(),
            trading: registry.account_id("Expenses:Trading").unwrap(),
            tbd: registry.account_id(TBD_ACCOUNT).unwrap(),
        };
        let mut out = Vec::new();
        import(source(rows).as_bytes(), registry, &accounts, &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn test_parse() {
        assert_eq!(
            parse(&source(&[BUY, CUSTODY_FEE])).unwrap(),
            vec![
                Line {
                    date: date(3, 25),
                    time: date(3, 25).and_hms_opt(9, 47, 33).unwrap(),
                    order: "288382377".into(),
                    kind: Kind::Buy,
                    kind_name: "Kauf".into(),
                    symbol: "VWRL".into(),
                    name: "Vanguard All-World".into(),
                    isin: "IE00B3RBWM25".into(),
                    quantity: dec("13.0"),
                    price: dec("128.98"),
                    fee: dec("14.37"),
                    // Thousands separators are dropped.
                    amount: dec("-1691.12"),
                    balance: dec("123.21"),
                    currency: "CHF".into(),
                },
                Line {
                    date: date(6, 30),
                    time: date(6, 30).and_hms_opt(12, 58, 5).unwrap(),
                    order: "00000000".into(),
                    kind: Kind::Fee,
                    kind_name: "Depotgebühren".into(),
                    symbol: "".into(),
                    name: "".into(),
                    isin: "".into(),
                    quantity: dec("1.0"),
                    price: dec("50.00"),
                    fee: dec("4.05"),
                    amount: dec("-54.05"),
                    balance: dec("15.11"),
                    currency: "CHF".into(),
                },
            ]
        );
    }

    #[test]
    fn test_parse_errors() {
        let err = |source: &str| parse(source).unwrap_err().to_string();
        assert!(err("").starts_with("unexpected end of file"));
        assert!(err("a;b;c\n").starts_with("invalid header"));
        assert!(err(&format!("{}\na;b;c\n", HEADER.join(";"))).starts_with("invalid line"));
        let broken = |i: usize, value: &'static str| {
            let mut row = BUY;
            row[i] = value;
            err(&source(&[row]))
        };
        assert!(broken(0, "25.03.2026").starts_with("invalid date"));
        assert!(broken(6, "x").starts_with("invalid quantity"));
        assert!(broken(7, "x").starts_with("invalid price"));
        assert!(broken(8, "x").starts_with("invalid fee"));
        assert!(broken(10, "x").starts_with("invalid amount"));
        assert!(broken(11, "x").starts_with("invalid balance"));
    }

    /// The header carries umlauts, so a Latin-1 export is only recognized
    /// once it is decoded.
    #[test]
    fn test_decode() {
        assert_eq!(decode(b"W\xe4hrung"), "Währung");
        assert_eq!(decode("Währung".as_bytes()), "Währung");
        let latin1 = source(&[CUSTODY_FEE])
            .chars()
            .map(|c| c as u8)
            .collect::<Vec<_>>();
        assert_eq!(parse(&decode(&latin1)).unwrap().len(), 1);
    }

    /// The cash amount is reported net of the fee: the journal books the
    /// gross trade against the trading account and the fee against the fee
    /// account, and the shares move the other way.
    #[test]
    fn test_import_buy() {
        assert_eq!(
            import_to_string(&[BUY]),
            "@performance(VWRL,CHF)\n\
             2026-03-25 \"288382377 Kauf 13 x VWRL Vanguard All-World IE00B3RBWM25 @ 128.98 CHF\"\n\
             Expenses:Trading  Assets:Swissquote         13 VWRL\n\
             Assets:Swissquote Expenses:Trading     1676.75 CHF\n\
             Assets:Swissquote Expenses:Fees          14.37 CHF\n\
             \n\
             2026-03-25 balance Assets:Swissquote 123.21 CHF\n"
        );
    }

    /// A sale moves the shares out of the account and the proceeds in.
    #[test]
    fn test_import_sell() {
        let mut sell = BUY;
        sell[2] = "Verkauf";
        sell[10] = "1'676.75";
        assert_eq!(
            import_to_string(&[sell]),
            "@performance(VWRL,CHF)\n\
             2026-03-25 \"288382377 Verkauf 13 x VWRL Vanguard All-World IE00B3RBWM25 @ 128.98 CHF\"\n\
             Assets:Swissquote Expenses:Trading          13 VWRL\n\
             Expenses:Trading  Assets:Swissquote    1691.12 CHF\n\
             Assets:Swissquote Expenses:Fees          14.37 CHF\n\
             \n\
             2026-03-25 balance Assets:Swissquote 123.21 CHF\n"
        );
    }

    /// A trade without a security cannot be booked.
    #[test]
    fn test_import_trade_without_symbol() {
        let mut row = BUY;
        row[3] = "";
        let registry = Rc::new(Registry::new());
        let account = registry.account_id("Assets:Swissquote").unwrap();
        let accounts = Accounts {
            account,
            dividend: account,
            interest: account,
            tax: account,
            fee: account,
            trading: account,
            tbd: account,
        };
        let err = import(
            source(&[row]).as_bytes(),
            registry,
            &accounts,
            &mut Vec::new(),
        )
        .unwrap_err()
        .to_string();
        assert_eq!(err, "Kauf on 2026-03-25 without a symbol");
    }

    /// The payment is reported net of the withholding tax deducted from it;
    /// both are booked, and the transaction is attributed to the position.
    #[test]
    fn test_import_dividend() {
        let dividend: Row = [
            "01-07-2026 16:00:54",
            "00000000",
            "Dividende",
            "VWRL",
            "Vanguard All-World",
            "IE00B3RBWM25",
            "1.0",
            "100.00",
            "15.00",
            "0.00",
            "85.00",
            "3'210.97",
            "USD",
        ];
        assert_eq!(
            import_to_string(&[dividend]),
            "@performance(VWRL)\n\
             2026-07-01 \"Dividende VWRL Vanguard All-World IE00B3RBWM25\"\n\
             Income:Dividends  Assets:Swissquote     100.00 USD\n\
             Assets:Swissquote Expenses:Tax           15.00 USD\n\
             \n\
             2026-07-01 balance Assets:Swissquote 3210.97 USD\n"
        );
    }

    /// The two legs share a timestamp and are booked as one transaction.
    #[test]
    fn test_import_exchange() {
        let credit: Row = [
            "25-03-2026 09:46:08",
            "288382741",
            "Forex-Gutschrift",
            "",
            "",
            "",
            "1.0",
            "1'776.32",
            "0.00",
            "0.00",
            "1'776.32",
            "1'814.33",
            "CHF",
        ];
        let mut debit = credit;
        debit[2] = "Forex-Belastung";
        debit[10] = "-2'273.45";
        debit[11] = "0.00";
        debit[12] = "USD";
        assert_eq!(
            import_to_string(&[credit, debit]),
            "@performance(CHF,USD)\n\
             2026-03-25 \"Forex-Gutschrift 1776.32 CHF / Forex-Belastung -2273.45 USD\"\n\
             Expenses:Trading  Assets:Swissquote    1776.32 CHF\n\
             Assets:Swissquote Expenses:Trading     2273.45 USD\n\
             \n\
             2026-03-25 balance Assets:Swissquote 1814.33 CHF\n\
             2026-03-25 balance Assets:Swissquote 0.00 USD\n"
        );
    }

    /// Legs which do not pair up one to one are booked on their own rather
    /// than guessed at: here, two credits in the same second.
    #[test]
    fn test_import_unmatched_exchange() {
        let credit: Row = [
            "25-03-2026 09:46:08",
            "288382741",
            "Forex-Gutschrift",
            "",
            "",
            "",
            "1.0",
            "100.00",
            "0.00",
            "0.00",
            "100.00",
            "100.00",
            "CHF",
        ];
        let mut other = credit;
        other[12] = "USD";
        let journal = import_to_string(&[credit, other]);
        assert_eq!(journal.matches("Forex-Gutschrift").count(), 2);
        assert!(
            journal.contains("Expenses:Trading  Assets:Swissquote     100.00 CHF"),
            "{journal}"
        );
    }

    /// Rows which only move cash: custody fees, which are performance-
    /// relevant for the portfolio as a whole rather than for one commodity,
    /// transfers to and from the outside world, interest, and a type the
    /// importer does not know.
    #[test]
    fn test_import_cash() {
        let mut deposit = CUSTODY_FEE;
        deposit[0] = "30-06-2026 13:00:00";
        deposit[2] = "Einzahlung";
        deposit[10] = "1'000.00";
        let mut interest = CUSTODY_FEE;
        interest[0] = "30-06-2026 14:00:00";
        interest[2] = "Zins";
        interest[10] = "0.19";
        let mut unknown = CUSTODY_FEE;
        unknown[0] = "30-06-2026 15:00:00";
        unknown[2] = "Stempelsteuer";
        unknown[10] = "-1.00";
        assert_eq!(
            import_to_string(&[CUSTODY_FEE, deposit, interest, unknown]),
            "@performance()\n\
             2026-06-30 \"Depotgebühren\"\n\
             Assets:Swissquote Expenses:Fees          54.05 CHF\n\
             \n\
             2026-06-30 \"Einzahlung\"\n\
             Expenses:TBD      Assets:Swissquote    1000.00 CHF\n\
             \n\
             @performance(CHF)\n\
             2026-06-30 \"Zins\"\n\
             Income:Interest   Assets:Swissquote       0.19 CHF\n\
             \n\
             2026-06-30 \"Stempelsteuer\"\n\
             Assets:Swissquote Expenses:TBD            1.00 CHF\n\
             \n\
             2026-06-30 balance Assets:Swissquote 15.11 CHF\n"
        );
    }

    /// The export lists the newest row first; the timestamp puts the rows of
    /// a day back in the order they were booked.
    #[test]
    fn test_import_orders_by_time() {
        let mut later = CUSTODY_FEE;
        later[0] = "30-06-2026 15:00:00";
        later[2] = "Auszahlung";
        let journal = import_to_string(&[later, CUSTODY_FEE]);
        let types = journal
            .lines()
            .filter(|l| l.starts_with("2026-06-30 \""))
            .collect::<Vec<_>>();
        assert_eq!(
            types,
            vec!["2026-06-30 \"Depotgebühren\"", "2026-06-30 \"Auszahlung\""]
        );
    }

    /// One assertion per currency, for the balance after the last row
    /// reported in it.
    #[test]
    fn test_import_asserts_last_balance_per_currency() {
        let row = |time: &'static str, balance: &'static str, currency: &'static str| {
            let mut row = CUSTODY_FEE;
            row[0] = time;
            row[11] = balance;
            row[12] = currency;
            row
        };
        let journal = import_to_string(&[
            row("30-06-2026 15:00:00", "10.00", "CHF"),
            row("30-06-2026 14:00:00", "20.00", "CHF"),
            row("29-06-2026 12:00:00", "30.00", "USD"),
        ]);
        let assertions = journal
            .lines()
            .filter(|l| l.contains("balance"))
            .collect::<Vec<_>>();
        assert_eq!(
            assertions,
            vec![
                "2026-06-29 balance Assets:Swissquote 30.00 USD",
                "2026-06-30 balance Assets:Swissquote 10.00 CHF",
            ]
        );
    }
}
