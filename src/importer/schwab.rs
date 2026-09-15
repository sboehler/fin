//! Importer for Charles Schwab accounts.
//!
//! Schwab exports the two accounts this importer covers in two different
//! layouts, which are told apart by their header:
//!
//! - the brokerage account, exported under "Accounts / History" by picking
//!   the account and the date range: eight columns, one row per leg;
//! - the equity awards account, exported from the Equity Awards Center:
//!   twenty-six columns, where a row without a date carries the lot details
//!   of the row above it and is ignored.
//!
//! The two overlap: the journal of an awards sale to the brokerage account is
//! reported by both exports, once from each side, so importing both files
//! yields that transfer twice.
//!
//! Cash amounts are reported net of commission, and the dividend of a
//! position is split over a payment row and a withholding tax row; both are
//! reassembled here.

use std::{collections::HashMap, error::Error, io::Write, path::PathBuf, rc::Rc};

use chrono::NaiveDate;
use clap::Args;
use csv::StringRecord;
use rust_decimal::Decimal;

use crate::model::{
    entities::{AccountID, Booking, CommodityID, Transaction},
    printer::Printer,
    registry::Registry,
};

/// Schwab reports all amounts in US dollars; neither export names a currency.
const CURRENCY: &str = "USD";

/// The account which receives the counter-postings of external cash
/// transfers and of vested awards, unless one is given. The user is expected
/// to replace it when reconciling the imported journal.
const TBD_ACCOUNT: &str = "Expenses:TBD";

/// The brokerage account export.
#[derive(Args)]
pub struct Command {
    source: PathBuf,

    /// The brokerage account.
    #[arg(short, long)]
    account: String,

    /// The account internal transfers are settled against, e.g. the Schwab
    /// awards account.
    #[arg(short = 'j', long)]
    transfer: String,

    /// The account external cash transfers are settled against.
    #[arg(short, long, default_value = TBD_ACCOUNT)]
    bank: String,

    /// The dividend income account.
    #[arg(short, long)]
    dividend: String,

    /// The withholding tax account.
    #[arg(short = 'w', long)]
    tax: String,

    /// The interest income account.
    #[arg(short, long)]
    interest: String,

    /// The trading gain / loss account.
    #[arg(short, long)]
    trading: String,

    /// The commission account.
    #[arg(short, long)]
    fee: String,
}

impl Command {
    pub fn run(&self, w: &mut impl Write) -> Result<(), Box<dyn Error>> {
        let source = std::fs::read_to_string(&self.source)?;
        let registry = Rc::new(Registry::new());
        let accounts = Accounts {
            account: registry.account_id(&self.account)?,
            transfer: registry.account_id(&self.transfer)?,
            bank: registry.account_id(&self.bank)?,
            dividend: registry.account_id(&self.dividend)?,
            tax: registry.account_id(&self.tax)?,
            interest: registry.account_id(&self.interest)?,
            trading: registry.account_id(&self.trading)?,
            fee: registry.account_id(&self.fee)?,
            // Awards cannot vest into the brokerage account.
            award: registry.account_id(TBD_ACCOUNT)?,
        };
        import(&source, Format::Individual, registry, &accounts, w)
    }
}

/// The equity awards account export, which carries none of the cash
/// management the brokerage account does.
#[derive(Args)]
pub struct AwardsCommand {
    source: PathBuf,

    /// The equity awards account.
    #[arg(short, long)]
    account: String,

    /// The income account vested awards are credited to.
    #[arg(short = 'r', long)]
    award: String,

    /// The account the sale proceeds are journalled to, i.e. the brokerage
    /// account.
    #[arg(short = 'j', long)]
    transfer: String,

    /// The trading gain / loss account.
    #[arg(short, long)]
    trading: String,

    /// The commission account.
    #[arg(short, long)]
    fee: String,
}

impl AwardsCommand {
    pub fn run(&self, w: &mut impl Write) -> Result<(), Box<dyn Error>> {
        let source = std::fs::read_to_string(&self.source)?;
        let registry = Rc::new(Registry::new());
        // The awards export carries no dividends, interest or external
        // transfers, so the accounts which would book them are never
        // reached: `Action` does not parse those rows in this format.
        let tbd = registry.account_id(TBD_ACCOUNT)?;
        let accounts = Accounts {
            account: registry.account_id(&self.account)?,
            award: registry.account_id(&self.award)?,
            transfer: registry.account_id(&self.transfer)?,
            trading: registry.account_id(&self.trading)?,
            fee: registry.account_id(&self.fee)?,
            bank: tbd,
            dividend: tbd,
            tax: tbd,
            interest: tbd,
        };
        import(&source, Format::Awards, registry, &accounts, w)
    }
}

struct Accounts {
    account: AccountID,
    transfer: AccountID,
    bank: AccountID,
    award: AccountID,
    dividend: AccountID,
    tax: AccountID,
    interest: AccountID,
    trading: AccountID,
    fee: AccountID,
}

/// Imports a Schwab export and writes the resulting journal to `w`: one
/// transaction per row, sorted by date, except for dividends, where the
/// payment and the withholding tax of the same position on the same day form
/// one transaction, and for vested awards, where all deposits of a day do.
fn import(
    source: &str,
    format: Format,
    registry: Rc<Registry>,
    accounts: &Accounts,
    w: &mut impl Write,
) -> Result<(), Box<dyn Error>> {
    let builder = Builder {
        registry: registry.clone(),
        accounts,
        currency: registry.commodity_id(CURRENCY)?,
    };
    let lines = parse(source, format)?;

    let mut transactions: Vec<Transaction> = Vec::new();
    // The transaction a `(date, symbol)` pair or a date has already opened.
    let mut dividends: HashMap<(NaiveDate, String), usize> = HashMap::new();
    let mut awards: HashMap<NaiveDate, usize> = HashMap::new();
    for line in &lines {
        use Action::*;
        match line.action {
            Buy | Sell | ReinvestShares => transactions.push(builder.trade(line)?),
            Interest => transactions.push(builder.interest(line)?),
            InternalCash | InternalShares | ExternalCash => {
                transactions.push(builder.transfer(line)?)
            }
            Award => match awards.get(&line.date) {
                Some(&i) => transactions[i]
                    .bookings
                    .extend(builder.award_bookings(line)?),
                None => {
                    awards.insert(line.date, transactions.len());
                    transactions.push(builder.award(line)?);
                }
            },
            Dividend | NraTax => {
                let key = (line.date, line.symbol.clone());
                match dividends.get(&key) {
                    // The payment is booked before the tax, whichever row
                    // came first.
                    Some(&i) => {
                        let bookings = builder.dividend_bookings(line)?;
                        let at = if line.action == Dividend {
                            0
                        } else {
                            transactions[i].bookings.len()
                        };
                        transactions[i].bookings.splice(at..at, bookings);
                    }
                    None => {
                        dividends.insert(key, transactions.len());
                        transactions.push(builder.dividend(line)?);
                    }
                }
            }
        }
    }
    // The exports list the newest row first; only the day is ordered here,
    // the order within it is left as the file has it.
    transactions.sort_by_key(|t| t.date);

    Printer::new(w, registry).transactions(&transactions)?;
    Ok(())
}

/// Turns rows into transactions.
struct Builder<'a> {
    registry: Rc<Registry>,
    accounts: &'a Accounts,
    currency: CommodityID,
}

impl Builder<'_> {
    /// A purchase, a sale, or the purchase leg of a dividend reinvestment.
    /// The legs which leave the brokerage account are booked first.
    fn trade(&self, line: &Line) -> Result<Transaction, Box<dyn Error>> {
        let Accounts {
            account,
            trading,
            fee,
            ..
        } = *self.accounts;
        let asset = self.registry.commodity_id(&line.symbol)?;
        let quantity = line.quantity()?;
        let sold = line.action == Action::Sell;
        let shares = Booking::create(
            trading,
            account,
            if sold { -quantity } else { quantity },
            asset,
            None,
        );
        // The amount is reported net of commission, while the journal books
        // the gross trade against the trading account and the commission
        // separately.
        let cash = Booking::create(
            trading,
            account,
            line.amount()? + line.commission,
            self.currency,
            None,
        );
        let commission = match line.commission.is_zero() {
            true => Vec::new(),
            false => Booking::create(fee, account, -line.commission, self.currency, None),
        };
        let mut bookings = Vec::new();
        if sold {
            bookings.extend(shares);
            bookings.extend(commission);
            bookings.extend(cash);
        } else {
            bookings.extend(cash);
            bookings.extend(commission);
            bookings.extend(shares);
        }
        let description = match line.action {
            Action::Buy => "Buy",
            Action::Sell => "Sell",
            _ => "Dividend Reinvestment",
        };
        Ok(Transaction {
            loc: None,
            date: line.date,
            description: Rc::new(description.to_string()),
            bookings,
            targets: Some(vec![self.currency, asset]),
        })
    }

    /// A dividend payment or a withholding tax adjustment. Both are
    /// attributed to the position they were paid on.
    fn dividend(&self, line: &Line) -> Result<Transaction, Box<dyn Error>> {
        Ok(Transaction {
            loc: None,
            date: line.date,
            description: Rc::new("Dividend".to_string()),
            bookings: self.dividend_bookings(line)?,
            targets: Some(vec![self.registry.commodity_id(&line.symbol)?]),
        })
    }

    fn dividend_bookings(&self, line: &Line) -> Result<Vec<Booking>, Box<dyn Error>> {
        let credit = match line.action {
            Action::Dividend => self.accounts.dividend,
            _ => self.accounts.tax,
        };
        Ok(Booking::create(
            credit,
            self.accounts.account,
            line.amount()?,
            self.currency,
            None,
        ))
    }

    /// Shares vesting out of an equity award. One deposit row is reported per
    /// award which vested, and the journal books the day's deposits together.
    fn award(&self, line: &Line) -> Result<Transaction, Box<dyn Error>> {
        Ok(Transaction {
            loc: None,
            date: line.date,
            description: Rc::new("Award".to_string()),
            bookings: self.award_bookings(line)?,
            targets: None,
        })
    }

    fn award_bookings(&self, line: &Line) -> Result<Vec<Booking>, Box<dyn Error>> {
        Ok(Booking::create(
            self.accounts.award,
            self.accounts.account,
            line.quantity()?,
            self.registry.commodity_id(&line.symbol)?,
            None,
        ))
    }

    fn interest(&self, line: &Line) -> Result<Transaction, Box<dyn Error>> {
        Ok(Transaction {
            loc: None,
            date: line.date,
            description: Rc::new("Interest".to_string()),
            bookings: Booking::create(
                self.accounts.interest,
                self.accounts.account,
                line.amount()?,
                self.currency,
                None,
            ),
            targets: None,
        })
    }

    /// A transfer of cash or shares to or from another account.
    fn transfer(&self, line: &Line) -> Result<Transaction, Box<dyn Error>> {
        let credit = match line.action {
            Action::ExternalCash => self.accounts.bank,
            _ => self.accounts.transfer,
        };
        let (quantity, commodity) = match line.action {
            Action::InternalShares => (line.quantity()?, self.registry.commodity_id(&line.symbol)?),
            _ => (line.amount()?, self.currency),
        };
        Ok(Transaction {
            loc: None,
            date: line.date,
            description: Rc::new("Transfer".to_string()),
            bookings: Booking::create(credit, self.accounts.account, quantity, commodity, None),
            targets: None,
        })
    }
}

/// The exports this importer understands, told apart by their header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    /// The brokerage account, with eight columns.
    Individual,
    /// The equity awards account, with twenty-six.
    Awards,
}

impl Format {
    fn detect(header: &StringRecord) -> Result<Format, Box<dyn Error>> {
        match (header.len(), Field::Date.read(header)) {
            (8, "Date") => Ok(Format::Individual),
            (26, "Date") => Ok(Format::Awards),
            _ => Err(format!("invalid header: {header:?}").into()),
        }
    }

    /// Names the export and the command which reads it.
    fn describe(self) -> &'static str {
        match self {
            Format::Individual => "the brokerage export (com.schwab)",
            Format::Awards => "the equity awards export (com.schwab.awards)",
        }
    }

    fn columns(self) -> usize {
        match self {
            Format::Individual => 8,
            Format::Awards => 26,
        }
    }

    /// The only column the two layouts disagree on.
    fn commission(self) -> usize {
        match self {
            Format::Individual => 6,
            Format::Awards => 5,
        }
    }
}

fn parse(source: &str, expected: Format) -> Result<Vec<Line>, Box<dyn Error>> {
    let mut records = csv::ReaderBuilder::new()
        .has_headers(false)
        // Report a row with the wrong number of columns as an invalid line
        // rather than letting the reader complain about the header.
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(source.as_bytes())
        .into_records();
    let format = match records.next().transpose()? {
        Some(rec) => Format::detect(&rec)?,
        None => return Err("unexpected end of file while looking for header".into()),
    };
    if format != expected {
        return Err(format!("this is {}, not {}", format.describe(), expected.describe()).into());
    }
    let mut lines = Vec::new();
    for rec in records {
        if let Some(line) = Line::parse(format, &rec?)? {
            lines.push(line);
        }
    }
    Ok(lines)
}

/// A parsed transaction row.
#[derive(Debug, PartialEq, Eq)]
struct Line {
    date: NaiveDate,
    action: Action,
    /// The position the row refers to, empty for cash-only rows.
    symbol: String,
    /// Absent on rows which move no shares.
    quantity: Option<Decimal>,
    /// The commission, zero if the row carries none. Always positive.
    commission: Decimal,
    /// The cash booked to the account, absent on rows which move no cash.
    amount: Option<Decimal>,
}

/// Columns both exports agree on, in file order.
#[derive(Clone, Copy)]
enum Field {
    Date = 0,
    Action = 1,
    Symbol = 2,
    Quantity = 4,
    Amount = 7,
}

impl Field {
    fn read(self, rec: &StringRecord) -> &str {
        rec.get(self as usize).unwrap_or_default()
    }
}

/// The row kinds this importer understands. Schwab uses one action per kind
/// of leg, so the action alone determines how a row is booked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Buy,
    Sell,
    /// The purchase leg of a dividend reinvestment.
    ReinvestShares,
    /// A dividend payment, however it is settled.
    Dividend,
    /// The withholding tax deducted from a dividend.
    NraTax,
    Interest,
    /// Cash moved between Schwab accounts.
    InternalCash,
    /// Shares moved between Schwab accounts.
    InternalShares,
    /// Cash moved to or from an outside bank.
    ExternalCash,
    /// Shares vesting out of an equity award.
    Award,
}

impl Action {
    fn parse(format: Format, value: &str) -> Result<Action, Box<dyn Error>> {
        use Format::*;
        Ok(match (format, value) {
            (Individual, "Buy") => Action::Buy,
            (Individual, "Sell") => Action::Sell,
            (Individual, "Reinvest Shares") => Action::ReinvestShares,
            (Individual, "Reinvest Dividend" | "Cash Dividend" | "Qualified Dividend") => {
                Action::Dividend
            }
            (Individual, "NRA Tax Adj") => Action::NraTax,
            (Individual, "Credit Interest") => Action::Interest,
            (Individual, "Journal") => Action::InternalCash,
            (Individual, "Journaled Shares") => Action::InternalShares,
            (Individual, "MoneyLink Transfer") => Action::ExternalCash,
            (Awards, "Deposit") => Action::Award,
            (Awards, "Sale") => Action::Sell,
            // The journal of the sale proceeds to the brokerage account.
            // The brokerage export reports the same transfer from the other
            // side, so importing both files yields it twice.
            (Awards, "Journal") => Action::InternalCash,
            _ => return Err(format!("unknown action {value:?}").into()),
        })
    }
}

impl Line {
    /// Parses a row, or returns `None` if it carries no transaction.
    fn parse(format: Format, rec: &StringRecord) -> Result<Option<Line>, Box<dyn Error>> {
        if rec.len() != format.columns() {
            return Err(format!("invalid line: {rec:?}").into());
        }
        let date = Field::Date.read(rec);
        // In the awards export a row without a date holds the lot details of
        // the row above it.
        if date.is_empty() && format == Format::Awards {
            return Ok(None);
        }
        let date = NaiveDate::parse_from_str(date, "%m/%d/%Y")
            .map_err(|e| format!("invalid date {date:?}: {e}"))?;
        let action =
            Action::parse(format, Field::Action.read(rec)).map_err(|e| format!("{e} on {date}"))?;
        let commission = rec.get(format.commission()).unwrap_or_default();
        Ok(Some(Line {
            date,
            action,
            symbol: Field::Symbol.read(rec).to_string(),
            quantity: parse_optional(Field::Quantity.read(rec))
                .map_err(|e| format!("invalid quantity on {date}: {e}"))?,
            commission: parse_optional(commission)
                .map_err(|e| format!("invalid commission on {date}: {e}"))?
                .unwrap_or_default(),
            amount: parse_optional(Field::Amount.read(rec))
                .map_err(|e| format!("invalid amount on {date}: {e}"))?,
        }))
    }

    fn quantity(&self) -> Result<Decimal, Box<dyn Error>> {
        self.quantity
            .ok_or_else(|| format!("missing quantity on {} {:?}", self.date, self.action).into())
    }

    fn amount(&self) -> Result<Decimal, Box<dyn Error>> {
        self.amount
            .ok_or_else(|| format!("missing amount on {} {:?}", self.date, self.action).into())
    }
}

/// Parses an amount as written by Schwab, e.g. `-$1,234.50` or `3,590.7908`.
/// The scale is kept as reported, so that the journal shows the same number
/// of decimals as the statement.
fn parse_optional(s: &str) -> Result<Option<Decimal>, Box<dyn Error>> {
    if s.is_empty() {
        return Ok(None);
    }
    Ok(Some(s.replace(['$', ','], "").parse()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    // The figures below are made up; the real statements are covered by the
    // golden tests in `testdata/private`.
    const HEADER: &str = "\"Date\",\"Action\",\"Symbol\",\"Description\",\"Quantity\",\"Price\",\"Fees & Comm\",\"Amount\"\n";

    const AWARDS_HEADER: &str = "\"Date\",\"Action\",\"Symbol\",\"Description\",\"Quantity\",\
\"FeesAndCommissions\",\"DisbursementElection\",\"Amount\",\"AwardDate\",\"AwardId\",\"VestDate\",\
\"VestFairMarketValue\",\"Type\",\"Shares\",\"SalePrice\",\"SubscriptionDate\",\
\"SubscriptionFairMarketValue\",\"PurchaseDate\",\"PurchasePrice\",\"PurchaseFairMarketValue\",\
\"DispositionType\",\"GrantId\",\"GrossProceeds\",\"TotalCostBasis\",\"RealizedGainLoss\",\
\"HoldingPeriod\"\n";

    fn import_with_header(
        format: Format,
        header: &str,
        rows: &str,
    ) -> Result<String, Box<dyn Error>> {
        let registry = Rc::new(Registry::new());
        let accounts = Accounts {
            account: registry.account_id("Assets:Schwab")?,
            transfer: registry.account_id("Assets:Awards")?,
            bank: registry.account_id("Assets:Bank")?,
            award: registry.account_id("Income:Awards")?,
            dividend: registry.account_id("Income:Dividends")?,
            tax: registry.account_id("Expenses:Tax")?,
            interest: registry.account_id("Income:Interest")?,
            trading: registry.account_id("Expenses:Trading")?,
            fee: registry.account_id("Expenses:Fees")?,
        };
        let mut out = Vec::new();
        import(
            &format!("{header}{rows}"),
            format,
            registry,
            &accounts,
            &mut out,
        )?;
        Ok(String::from_utf8(out)?)
    }

    fn import_to_string(rows: &str) -> Result<String, Box<dyn Error>> {
        import_with_header(Format::Individual, HEADER, rows)
    }

    /// Builds an awards row, padding the lot detail columns which are only
    /// filled in on the detail rows.
    fn awards_row(columns: &[&str]) -> String {
        let mut row = columns.to_vec();
        row.resize(26, "");
        row.iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(",")
            + "\n"
    }

    fn import_awards_to_string(rows: &str) -> Result<String, Box<dyn Error>> {
        import_with_header(Format::Awards, AWARDS_HEADER, rows)
    }

    #[test]
    fn test_buy() {
        // Cash leaves the account first, then the shares arrive.
        assert_eq!(
            import_to_string(
                "\"01/15/2024\",\"Buy\",\"AAA\",\"FUND A\",\"10\",\"$12.50\",\"\",\"-$125.00\"\n"
            )
            .unwrap(),
            "@performance(USD,AAA)\n\
             2024-01-15 \"Buy\"\n\
             Assets:Schwab    Expenses:Trading     125.00 USD\n\
             Expenses:Trading Assets:Schwab            10 AAA\n"
        );
    }

    #[test]
    fn test_sell_books_the_commission_separately() {
        // The amount is net of the commission: the trading account is
        // credited with 119.75 + 0.25.
        assert_eq!(
            import_to_string(
                "\"02/20/2024\",\"Sell\",\"BBB\",\"FUND B\",\"4\",\"$30.00\",\"$0.25\",\"$119.75\"\n"
            )
            .unwrap(),
            "@performance(USD,BBB)\n\
             2024-02-20 \"Sell\"\n\
             Assets:Schwab    Expenses:Trading          4 BBB\n\
             Assets:Schwab    Expenses:Fees          0.25 USD\n\
             Expenses:Trading Assets:Schwab        120.00 USD\n"
        );
    }

    #[test]
    fn test_dividend_joins_the_withholding_tax() {
        // Three rows on one day: the payment and its tax become a single
        // transaction, whose payment leg is booked first whichever row came
        // first; the reinvestment stays separate.
        assert_eq!(
            import_to_string(
                "\"03/10/2024\",\"Reinvest Shares\",\"AAA\",\"FUND A\",\"1.6\",\"$10.625\",\"\",\"-$17.00\"\n\
                 \"03/10/2024\",\"Reinvest Dividend\",\"AAA\",\"FUND A\",\"\",\"\",\"\",\"$20.00\"\n\
                 \"03/10/2024\",\"NRA Tax Adj\",\"AAA\",\"FUND A\",\"\",\"\",\"\",\"-$3.00\"\n"
            )
            .unwrap(),
            "@performance(USD,AAA)\n\
             2024-03-10 \"Dividend Reinvestment\"\n\
             Assets:Schwab    Expenses:Trading      17.00 USD\n\
             Expenses:Trading Assets:Schwab           1.6 AAA\n\
             \n\
             @performance(AAA)\n\
             2024-03-10 \"Dividend\"\n\
             Income:Dividends Assets:Schwab         20.00 USD\n\
             Assets:Schwab    Expenses:Tax           3.00 USD\n"
        );
    }

    #[test]
    fn test_cash_dividend_without_tax() {
        assert_eq!(
            import_to_string(
                "\"04/05/2024\",\"Cash Dividend\",\"BBB\",\"FUND B\",\"\",\"\",\"\",\"$8.40\"\n"
            )
            .unwrap(),
            "@performance(BBB)\n\
             2024-04-05 \"Dividend\"\n\
             Income:Dividends Assets:Schwab          8.40 USD\n"
        );
    }

    #[test]
    fn test_transfers() {
        // Internal transfers of cash and shares settle against the transfer
        // account, MoneyLink transfers against the bank account.
        assert_eq!(
            import_to_string(
                "\"05/02/2024\",\"MoneyLink Transfer\",\"\",\"TFR OUTSIDE BANK\",\"\",\"\",\"\",\"-$500.00\"\n\
                 \"06/03/2024\",\"Journaled Shares\",\"AAA\",\"FUND A\",\"12.5\",\"$11.00\",\"\",\"\"\n\
                 \"07/04/2024\",\"Journal\",\"\",\"JOURNAL FRM ...000\",\"\",\"\",\"\",\"$250.00\"\n"
            )
            .unwrap(),
            "2024-05-02 \"Transfer\"\n\
             Assets:Schwab Assets:Bank       500.00 USD\n\
             \n\
             2024-06-03 \"Transfer\"\n\
             Assets:Awards Assets:Schwab       12.5 AAA\n\
             \n\
             2024-07-04 \"Transfer\"\n\
             Assets:Awards Assets:Schwab     250.00 USD\n"
        );
    }

    #[test]
    fn test_interest() {
        assert_eq!(
            import_to_string(
                "\"08/09/2024\",\"Credit Interest\",\"\",\"SCHWAB1 INT\",\"\",\"\",\"\",\"$0.03\"\n"
            )
            .unwrap(),
            "2024-08-09 \"Interest\"\n\
             Income:Interest Assets:Schwab         0.03 USD\n"
        );
    }

    #[test]
    fn test_awards_export() {
        // The deposits of a day form one award, which the export lists
        // before the sale they funded. The lot detail row carries no
        // transaction; the journal of the proceeds does, and the brokerage
        // export reports the same transfer from the other side.
        let rows = awards_row(&[
            "09/12/2024",
            "Journal",
            "AAA",
            "Journal To Account ...000",
            "",
            "",
            "",
            "-$200.00",
        ]) + &awards_row(&["09/11/2024", "Deposit", "AAA", "RS", "3.5"])
            + &awards_row(&[])
            + &awards_row(&["09/11/2024", "Deposit", "AAA", "RS", "7.25"])
            + &awards_row(&[
                "09/11/2024",
                "Sale",
                "AAA",
                "Share Sale",
                "10.75",
                "$0.10",
                "Journal",
                "$199.90",
            ]);
        assert_eq!(
            import_awards_to_string(&rows).unwrap(),
            "2024-09-11 \"Award\"\n\
             Income:Awards    Assets:Schwab           3.5 AAA\n\
             Income:Awards    Assets:Schwab          7.25 AAA\n\
             \n\
             @performance(USD,AAA)\n\
             2024-09-11 \"Sell\"\n\
             Assets:Schwab    Expenses:Trading      10.75 AAA\n\
             Assets:Schwab    Expenses:Fees          0.10 USD\n\
             Expenses:Trading Assets:Schwab        200.00 USD\n\
             \n\
             2024-09-12 \"Transfer\"\n\
             Assets:Schwab    Assets:Awards        200.00 USD\n"
        );
    }

    #[test]
    fn test_actions_are_read_per_format() {
        // The two exports name their actions differently: neither reads
        // the other's vocabulary.
        assert_eq!(
            import_awards_to_string(&awards_row(&[
                "09/11/2024",
                "Buy",
                "AAA",
                "",
                "1",
                "",
                "",
                "-$1"
            ]))
            .unwrap_err()
            .to_string(),
            "unknown action \"Buy\" on 2024-09-11"
        );
        assert_eq!(
            import_to_string("\"09/11/2024\",\"Deposit\",\"AAA\",\"RS\",\"3.5\",\"\",\"\",\"\"\n")
                .unwrap_err()
                .to_string(),
            "unknown action \"Deposit\" on 2024-09-11"
        );
    }

    #[test]
    fn test_parse_errors() {
        let err = |rows: &str| import_to_string(rows).unwrap_err().to_string();
        assert_eq!(
            err("\"01/15/2024\",\"Wire Received\",\"\",\"\",\"\",\"\",\"\",\"$1\"\n"),
            "unknown action \"Wire Received\" on 2024-01-15"
        );
        assert_eq!(
            err("\"13/15/2024\",\"Buy\",\"AAA\",\"\",\"1\",\"$1\",\"\",\"-$1\"\n"),
            "invalid date \"13/15/2024\": input is out of range"
        );
        assert_eq!(
            err("\"01/15/2024\",\"Buy\",\"AAA\",\"\",\"\",\"$1\",\"\",\"-$1\"\n"),
            "missing quantity on 2024-01-15 Buy"
        );
        assert_eq!(
            err("\"01/15/2024\",\"Buy\",\"AAA\",\"\",\"1\",\"$1\",\"\",\"\"\n"),
            "missing amount on 2024-01-15 Buy"
        );
        // A row with the wrong number of columns is reported, not skipped.
        assert!(err("\"01/15/2024\",\"Buy\"\n").starts_with("invalid line:"));
    }

    #[test]
    fn test_awards_rows_must_have_all_columns() {
        // A short row is reported rather than read with shifted columns.
        let err = import_awards_to_string("\"09/11/2024\",\"Deposit\",\"AAA\"\n")
            .unwrap_err()
            .to_string();
        assert!(err.starts_with("invalid line:"), "{err}");
    }

    #[test]
    fn test_header_is_checked() {
        let err = parse("\"Datum\",\"Aktion\"\n", Format::Individual)
            .unwrap_err()
            .to_string();
        assert!(err.starts_with("invalid header:"), "{err}");
        assert_eq!(
            parse("", Format::Individual).unwrap_err().to_string(),
            "unexpected end of file while looking for header"
        );
    }

    #[test]
    fn test_each_command_reads_only_its_own_export() {
        // Pointing a command at the other export fails on the header rather
        // than reading the columns at the wrong offsets.
        assert_eq!(
            import_with_header(Format::Individual, AWARDS_HEADER, "")
                .unwrap_err()
                .to_string(),
            "this is the equity awards export (com.schwab.awards), \
             not the brokerage export (com.schwab)"
        );
        assert_eq!(
            import_with_header(Format::Awards, HEADER, "")
                .unwrap_err()
                .to_string(),
            "this is the brokerage export (com.schwab), \
             not the equity awards export (com.schwab.awards)"
        );
    }

    #[test]
    fn test_format_is_detected_from_the_header() {
        assert_eq!(
            Format::detect(&StringRecord::from(vec!["Date"; 8])).unwrap(),
            Format::Individual
        );
        assert_eq!(
            Format::detect(&StringRecord::from(vec!["Date"; 26])).unwrap(),
            Format::Awards
        );
        // A header of an unknown width is not guessed at.
        let err = Format::detect(&StringRecord::from(vec!["Date"; 9]))
            .unwrap_err()
            .to_string();
        assert!(err.starts_with("invalid header:"), "{err}");
    }
}
