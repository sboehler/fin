//! Importer for Interactive Brokers activity statements.
//!
//! In the account manager web UI, go to "Reports" and download an
//! "Activity" statement for the desired period (under "Default Statements").
//! Select CSV as the file format.
//!
//! The statement is a concatenation of sections. Every record starts with the
//! section name and a discriminator (`Header`, `Data`, `Total`, ...), followed
//! by section-specific columns. Only the `Data` records of the sections
//! listed in [`Section`] are imported; everything else is ignored.

use std::{error::Error, io::Write, path::PathBuf, rc::Rc};

use chrono::NaiveDate;
use clap::Args;
use csv::StringRecord;
use regex::Regex;
use rust_decimal::{Decimal, RoundingStrategy};

use crate::model::{
    entities::{AccountID, Assertion, Booking, CommodityID, Transaction},
    printer::Printer,
    registry::Registry,
};

/// The account which receives the counter-postings of deposits and
/// withdrawals. The user is expected to replace it when reconciling
/// the imported journal.
const TBD_ACCOUNT: &str = "Equity:TBD";

#[derive(Args)]
pub struct Command {
    source: PathBuf,

    /// The brokerage account.
    #[arg(short, long)]
    account: String,

    /// The dividend income account.
    #[arg(short, long)]
    dividend: String,

    /// The interest expense account.
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

    /// The account for rounding differences on cash balances.
    #[arg(short, long)]
    rounding: String,
}

impl Command {
    pub fn run(&self, w: &mut impl Write) -> Result<(), Box<dyn Error>> {
        let source = std::fs::read_to_string(&self.source)?;
        let registry = Rc::new(Registry::new());
        let accounts = Accounts {
            account: registry.account_id(&self.account)?,
            dividend: registry.account_id(&self.dividend)?,
            interest: registry.account_id(&self.interest)?,
            tax: registry.account_id(&self.tax)?,
            fee: registry.account_id(&self.fee)?,
            trading: registry.account_id(&self.trading)?,
            rounding: registry.account_id(&self.rounding)?,
            tbd: registry.account_id(TBD_ACCOUNT)?,
        };
        import(&source, registry, &accounts, w)
    }
}

struct Accounts {
    account: AccountID,
    dividend: AccountID,
    interest: AccountID,
    tax: AccountID,
    fee: AccountID,
    trading: AccountID,
    rounding: AccountID,
    tbd: AccountID,
}

/// Imports an activity statement and writes the resulting journal to `w`:
/// one transaction per trade, transfer, dividend, withholding tax, interest
/// and fee entry, sorted by date, followed by balance assertions for the
/// open positions and cash balances at the end of the statement period.
///
/// Cash amounts are booked in cents, while the statement reports them with
/// up to nine decimals. The rounding differences accumulated over the period
/// are booked to the rounding account at the period end, so that the cash
/// assertions hold given the balances at the start of the period.
fn import(
    source: &str,
    registry: Rc<Registry>,
    accounts: &Accounts,
    w: &mut impl Write,
) -> Result<(), Box<dyn Error>> {
    let statement = Statement::parse(source)?;
    let builder = Builder {
        registry: registry.clone(),
        accounts,
        base_currency: registry.commodity_id(&statement.base_currency)?,
    };

    let mut transactions = Vec::new();
    for trade in &statement.trades {
        transactions.push(builder.trade(trade)?);
    }
    for cash in &statement.transfers {
        transactions.push(builder.transfer(cash)?);
    }
    for cash in &statement.dividends {
        transactions.push(builder.dividend(cash, accounts.dividend)?);
    }
    for cash in &statement.withholding_taxes {
        transactions.push(builder.dividend(cash, accounts.tax)?);
    }
    for cash in &statement.interest {
        transactions.push(builder.interest(cash)?);
    }
    for cash in &statement.fees {
        transactions.push(builder.fee(cash)?);
    }

    let mut assertions = Vec::new();
    for position in &statement.positions {
        assertions.push(Assertion {
            loc: None,
            date: statement.period_end,
            account: accounts.account,
            balance: position.quantity,
            commodity: registry.commodity_id(&position.commodity)?,
        });
    }

    for balance in &statement.cash_balances {
        let currency = registry.commodity_id(&balance.currency)?;
        let booked = transactions
            .iter()
            .flat_map(|t| &t.bookings)
            .filter(|b| b.account == accounts.account && b.commodity == currency)
            .map(|b| b.quantity)
            .sum::<Decimal>();
        // The assertion is what must hold in the end; fall back to the cash
        // report if the currency has no forex balance entry.
        let end = assertions
            .iter()
            .find(|a| a.commodity == currency)
            .map(|a| a.balance)
            .unwrap_or(balance.end);
        let difference = end - balance.start - booked;
        if !difference.is_zero() {
            transactions.push(builder.rounding(
                &balance.currency,
                difference,
                statement.period_end,
            )?);
        }
    }
    transactions.sort_by_key(|t| t.date);

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

/// Turns statement entries into transactions.
struct Builder<'a> {
    registry: Rc<Registry>,
    accounts: &'a Accounts,
    base_currency: CommodityID,
}

impl Builder<'_> {
    fn trade(&self, trade: &Trade) -> Result<Transaction, Box<dyn Error>> {
        let Accounts {
            account,
            trading,
            fee,
            ..
        } = *self.accounts;
        let currency = self.commodity(&trade.currency)?;
        let asset = self.commodity(&trade.asset)?;
        let side = if trade.quantity.is_sign_positive() {
            "Buy"
        } else {
            "Sell"
        };
        let description = format!(
            "{side} {} {} @ {} {}",
            trade.quantity.abs(),
            trade.asset,
            trade.price,
            trade.currency
        );
        let mut bookings = Vec::new();
        bookings.extend(Booking::create(
            trading,
            account,
            trade.quantity,
            asset,
            None,
        ));
        bookings.extend(Booking::create(
            trading,
            account,
            trade.proceeds,
            currency,
            None,
        ));
        // Stock commissions are charged in the trade currency, forex
        // commissions in the base currency.
        match trade.category {
            AssetCategory::Stocks => {
                bookings.extend(Booking::create(fee, account, trade.fee, currency, None));
            }
            AssetCategory::Forex if !trade.fee.is_zero() => {
                bookings.extend(Booking::create(
                    fee,
                    account,
                    trade.fee,
                    self.base_currency,
                    None,
                ));
            }
            AssetCategory::Forex => {}
        }
        Ok(Transaction {
            loc: None,
            date: trade.date,
            description: Rc::new(description),
            bookings,
            targets: Some(vec![asset, currency]),
        })
    }

    fn transfer(&self, cash: &Cash) -> Result<Transaction, Box<dyn Error>> {
        let Accounts { account, tbd, .. } = *self.accounts;
        let currency = self.commodity(&cash.currency)?;
        let kind = if cash.amount.is_sign_positive() {
            "Deposit"
        } else {
            "Withdraw"
        };
        Ok(Transaction {
            loc: None,
            date: cash.date,
            description: Rc::new(format!("{kind} {} {}", cash.amount.abs(), cash.currency)),
            bookings: Booking::create(tbd, account, cash.amount, currency, None),
            targets: None,
        })
    }

    /// Dividends and withholding taxes are attributed to the security named
    /// at the start of their description, e.g. `VTI(US9229087690) Cash Dividend ...`.
    fn dividend(&self, cash: &Cash, credit: AccountID) -> Result<Transaction, Box<dyn Error>> {
        let currency = self.commodity(&cash.currency)?;
        let security = Regex::new("^[A-Za-z0-9]+")
            .unwrap()
            .find(&cash.description)
            .ok_or_else(|| format!("no security in description {:?}", cash.description))?;
        let security = self.commodity(security.as_str())?;
        Ok(Transaction {
            loc: None,
            date: cash.date,
            description: Rc::new(cash.description.clone()),
            bookings: Booking::create(credit, self.accounts.account, cash.amount, currency, None),
            targets: Some(vec![security]),
        })
    }

    fn interest(&self, cash: &Cash) -> Result<Transaction, Box<dyn Error>> {
        let currency = self.commodity(&cash.currency)?;
        Ok(Transaction {
            loc: None,
            date: cash.date,
            description: Rc::new(cash.description.clone()),
            bookings: Booking::create(
                self.accounts.interest,
                self.accounts.account,
                cash.amount,
                currency,
                None,
            ),
            targets: Some(vec![currency]),
        })
    }

    fn fee(&self, cash: &Cash) -> Result<Transaction, Box<dyn Error>> {
        let currency = self.commodity(&cash.currency)?;
        Ok(Transaction {
            loc: None,
            date: cash.date,
            description: Rc::new(cash.description.clone()),
            bookings: Booking::create(
                self.accounts.fee,
                self.accounts.account,
                cash.amount,
                currency,
                None,
            ),
            targets: None,
        })
    }

    fn rounding(
        &self,
        currency: &str,
        difference: Decimal,
        date: NaiveDate,
    ) -> Result<Transaction, Box<dyn Error>> {
        Ok(Transaction {
            loc: None,
            date,
            description: Rc::new(format!("Rounding differences {currency}")),
            bookings: Booking::create(
                self.accounts.rounding,
                self.accounts.account,
                difference,
                self.commodity(currency)?,
                None,
            ),
            targets: None,
        })
    }

    fn commodity(&self, name: &str) -> Result<CommodityID, Box<dyn Error>> {
        Ok(self.registry.commodity_id(name)?)
    }
}

/// The imported parts of an activity statement.
#[derive(Debug, Default, PartialEq, Eq)]
struct Statement {
    base_currency: String,
    period_end: NaiveDate,
    trades: Vec<Trade>,
    transfers: Vec<Cash>,
    dividends: Vec<Cash>,
    withholding_taxes: Vec<Cash>,
    interest: Vec<Cash>,
    fees: Vec<Cash>,
    /// Open positions and cash balances at the end of the period.
    positions: Vec<Position>,
    /// Cash balances at the start and end of the period, per currency.
    cash_balances: Vec<CashBalance>,
}

#[derive(Debug, PartialEq, Eq)]
struct Trade {
    category: AssetCategory,
    date: NaiveDate,
    /// The traded asset: a stock symbol, or the bought currency of a forex trade.
    asset: String,
    /// The currency in which the asset was paid for.
    currency: String,
    quantity: Decimal,
    price: Decimal,
    proceeds: Decimal,
    fee: Decimal,
}

#[derive(Debug, PartialEq, Eq)]
enum AssetCategory {
    Stocks,
    Forex,
}

/// A cash movement in a single currency.
#[derive(Debug, PartialEq, Eq)]
struct Cash {
    date: NaiveDate,
    currency: String,
    description: String,
    amount: Decimal,
}

#[derive(Debug, PartialEq, Eq)]
struct Position {
    commodity: String,
    quantity: Decimal,
}

/// Starting and ending cash of a currency, in cents.
#[derive(Debug, PartialEq, Eq)]
struct CashBalance {
    currency: String,
    start: Decimal,
    end: Decimal,
}

/// The statement sections which are imported, with the columns they are
/// read from (after the section name and discriminator).
enum Section {
    Statement,
    AccountInformation,
    Trades,
    DepositsWithdrawals,
    Dividends,
    WithholdingTax,
    Interest,
    Fees,
    OpenPositions,
    ForexBalances,
    CashReport,
}

impl Section {
    fn from_name(name: &str) -> Option<Section> {
        Some(match name {
            "Statement" => Section::Statement,
            "Account Information" => Section::AccountInformation,
            "Trades" => Section::Trades,
            "Deposits & Withdrawals" => Section::DepositsWithdrawals,
            "Dividends" => Section::Dividends,
            "Withholding Tax" => Section::WithholdingTax,
            "Interest" => Section::Interest,
            "Fees" => Section::Fees,
            "Open Positions" => Section::OpenPositions,
            "Forex Balances" => Section::ForexBalances,
            "Cash Report" => Section::CashReport,
            _ => return None,
        })
    }
}

/// Column positions within a `Trades` record.
#[allow(dead_code)]
#[derive(Clone, Copy)]
enum TradeField {
    Section,
    Discriminator,
    DataDiscriminator,
    AssetCategory,
    Currency,
    Symbol,
    DateTime,
    Quantity,
    Price,
    ClosePrice,
    Proceeds,
    Fee,
}

/// Column positions within a cash section record (deposits, dividends,
/// withholding taxes, interest). The `Fees` section has an additional
/// subtitle column before these.
#[derive(Clone, Copy)]
enum CashField {
    Currency = 2,
    Date,
    Description,
    Amount,
}

/// Column positions within an `Open Positions` record.
#[derive(Clone, Copy)]
enum PositionField {
    Symbol = 5,
    Quantity,
}

/// Column positions within a `Forex Balances` record.
#[derive(Clone, Copy)]
enum ForexBalanceField {
    Currency = 4,
    Quantity,
}

/// Column positions within a `Cash Report` record.
#[derive(Clone, Copy)]
enum CashReportField {
    Label = 2,
    Currency,
    Total,
}

/// A record with named column access; missing columns read as empty.
struct Record<'a>(&'a StringRecord);

impl Record<'_> {
    fn get(&self, i: usize) -> &str {
        self.0.get(i).unwrap_or_default()
    }

    fn section(&self) -> &str {
        self.get(0)
    }

    fn is_data(&self) -> bool {
        self.get(1) == "Data"
    }

    /// Cash sections report per-currency totals as `Data` records with
    /// `Total`, `Total in CHF`, ... in the currency column.
    fn is_total(&self) -> bool {
        self.get(2).starts_with("Total")
    }

    fn trade(&self, f: TradeField) -> &str {
        self.get(f as usize)
    }

    fn cash(&self, f: CashField, offset: usize) -> &str {
        self.get(f as usize + offset)
    }
}

impl Statement {
    fn parse(source: &str) -> Result<Statement, Box<dyn Error>> {
        let mut statement = Statement::default();
        let mut period_end = None;
        let reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .from_reader(source.as_bytes());
        for rec in reader.into_records() {
            let rec = rec?;
            let rec = Record(&rec);
            let Some(section) = Section::from_name(rec.section()) else {
                continue;
            };
            if !rec.is_data() {
                continue;
            }
            match section {
                Section::Statement if rec.get(2) == "Period" => {
                    period_end = Some(parse_period_end(rec.get(3))?);
                }
                Section::AccountInformation if rec.get(2) == "Base Currency" => {
                    statement.base_currency = rec.get(3).to_string();
                }
                Section::Trades if rec.trade(TradeField::DataDiscriminator) == "Order" => {
                    if let Some(trade) = parse_trade(&rec)? {
                        statement.trades.push(trade);
                    }
                }
                Section::DepositsWithdrawals if !rec.is_total() => {
                    statement.transfers.push(parse_cash(&rec, 0)?);
                }
                Section::Dividends if !rec.is_total() => {
                    statement.dividends.push(parse_cash(&rec, 0)?);
                }
                Section::WithholdingTax if !rec.is_total() => {
                    statement.withholding_taxes.push(parse_cash(&rec, 0)?);
                }
                Section::Interest if !rec.is_total() => {
                    statement.interest.push(parse_cash(&rec, 0)?);
                }
                Section::Fees if rec.get(2) == "Other Fees" => {
                    statement.fees.push(parse_cash(&rec, 1)?);
                }
                Section::OpenPositions if rec.get(2) == "Summary" => {
                    statement.positions.push(Position {
                        commodity: rec.get(PositionField::Symbol as usize).to_string(),
                        quantity: parse_decimal(rec.get(PositionField::Quantity as usize))?,
                    });
                }
                Section::CashReport => {
                    parse_cash_report(&rec, &mut statement.cash_balances)?;
                }
                Section::ForexBalances if rec.get(2) == "Forex" => {
                    statement.positions.push(Position {
                        commodity: rec.get(ForexBalanceField::Currency as usize).to_string(),
                        quantity: parse_rounded(rec.get(ForexBalanceField::Quantity as usize))?,
                    });
                }
                _ => {}
            }
        }
        if statement.base_currency.is_empty() {
            return Err("no base currency found".into());
        }
        statement.period_end = period_end.ok_or("no statement period found")?;
        Ok(statement)
    }
}

/// Parses the end date of a period like `January 2, 2025 - December 31, 2025`.
fn parse_period_end(s: &str) -> Result<NaiveDate, Box<dyn Error>> {
    let end = s.rsplit(" - ").next().unwrap_or_default();
    Ok(NaiveDate::parse_from_str(end.trim(), "%B %d, %Y")
        .map_err(|e| format!("invalid period {s:?}: {e}"))?)
}

/// Parses a trade; returns `None` for asset categories which are not imported.
fn parse_trade(rec: &Record) -> Result<Option<Trade>, Box<dyn Error>> {
    let category = match rec.trade(TradeField::AssetCategory) {
        "Stocks" => AssetCategory::Stocks,
        "Forex" => AssetCategory::Forex,
        _ => return Ok(None),
    };
    let symbol = rec.trade(TradeField::Symbol);
    let asset = match category {
        AssetCategory::Stocks => symbol,
        // Forex symbols are pairs like USD.CHF: USD bought, paid in CHF.
        AssetCategory::Forex => symbol.split('.').next().unwrap_or_default(),
    };
    // The date/time column reads `2025-01-02, 10:00:00`.
    let date = rec.trade(TradeField::DateTime);
    let date = parse_date(date.get(..10).unwrap_or(date))?;
    // Share quantities are exact; a forex quantity is a cash amount.
    let quantity = match category {
        AssetCategory::Stocks => parse_decimal(rec.trade(TradeField::Quantity))?,
        AssetCategory::Forex => parse_rounded(rec.trade(TradeField::Quantity))?,
    };
    Ok(Some(Trade {
        category,
        date,
        asset: asset.to_string(),
        currency: rec.trade(TradeField::Currency).to_string(),
        quantity,
        price: parse_decimal(rec.trade(TradeField::Price))?,
        proceeds: parse_rounded(rec.trade(TradeField::Proceeds))?,
        fee: parse_rounded(rec.trade(TradeField::Fee))?,
    }))
}

/// Collects the `Starting Cash` and `Ending Cash` rows per currency. The
/// section also carries a `Base Currency Summary` over all currencies, which
/// is skipped.
fn parse_cash_report(rec: &Record, balances: &mut Vec<CashBalance>) -> Result<(), Box<dyn Error>> {
    let label = rec.get(CashReportField::Label as usize);
    let currency = rec.get(CashReportField::Currency as usize);
    if !matches!(label, "Starting Cash" | "Ending Cash") || currency == "Base Currency Summary" {
        return Ok(());
    }
    let total = parse_rounded(rec.get(CashReportField::Total as usize))?;
    let balance = match balances.iter_mut().find(|b| b.currency == currency) {
        Some(b) => b,
        None => {
            balances.push(CashBalance {
                currency: currency.to_string(),
                start: Decimal::ZERO,
                end: Decimal::ZERO,
            });
            balances.last_mut().unwrap()
        }
    };
    if label == "Starting Cash" {
        balance.start = total;
    } else {
        balance.end = total;
    }
    Ok(())
}

fn parse_cash(rec: &Record, offset: usize) -> Result<Cash, Box<dyn Error>> {
    Ok(Cash {
        date: parse_date(rec.cash(CashField::Date, offset))?,
        currency: rec.cash(CashField::Currency, offset).to_string(),
        description: rec
            .cash(CashField::Description, offset)
            // Descriptions are printed as quoted strings, which cannot contain quotes.
            .replace('"', "'"),
        amount: parse_decimal(rec.cash(CashField::Amount, offset))?,
    })
}

fn parse_date(s: &str) -> Result<NaiveDate, Box<dyn Error>> {
    Ok(NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|e| format!("invalid date {s:?}: {e}"))?)
}

/// Parses an amount with optional thousands separators, e.g. `1,234.50`.
/// Empty amounts read as zero.
fn parse_decimal(s: &str) -> Result<Decimal, Box<dyn Error>> {
    if s.is_empty() {
        return Ok(Decimal::ZERO);
    }
    Ok(s.replace(',', "")
        .parse()
        .map_err(|e| format!("invalid amount {s:?}: {e}"))?)
}

/// Parses an amount and rounds it to cents; the statement reports some
/// quantities with more precision than is booked.
fn parse_rounded(s: &str) -> Result<Decimal, Box<dyn Error>> {
    Ok(parse_decimal(s)?.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn dec(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn test_parse_statement() {
        let source = "\
\u{feff}Statement,Header,Field Name,Field Value
Statement,Data,Period,\"January 1, 2025 - December 31, 2025\"
Account Information,Header,Field Name,Field Value
Account Information,Data,Base Currency,CHF
Trades,Header,DataDiscriminator,Asset Category,Currency,Symbol,Date/Time,Quantity,T. Price,C. Price,Proceeds,Comm/Fee,Basis,Realized P/L,Realized P/L %,MTM P/L,Code
Trades,Data,Order,Stocks,USD,VTI,\"2025-03-03, 09:30:00\",10,250.123,251,-2501.23,-1,2502.23,0,0,8.77,O
Trades,SubTotal,,Stocks,USD,VTI,,10,,,-2501.23,-1,2502.23,0,,8.77,
Trades,Data,Order,Forex,CHF,USD.CHF,\"2025-03-01, 10:00:00\",\"1,000\",0.9,,-900.004,-1.8,,,,0,
Trades,Data,Order,Forex,CHF,USD.CHF,\"2025-03-02, 10:00:00\",\"1,000\",0.9,,-900.004,-1.8,,,,0,
Trades,Data,Order,Bonds,USD,X,\"2025-03-01, 10:00:00\",1,1,,-1,0,,,,0,
Deposits & Withdrawals,Header,Currency,Settle Date,Description,Amount
Deposits & Withdrawals,Data,CHF,2025-02-01,Electronic Fund Transfer,\"5,000\"
Deposits & Withdrawals,Data,Total,,,5000
Dividends,Header,Currency,Date,Description,Amount
Dividends,Data,USD,2025-04-01,VTI(US9229087690) Cash Dividend USD 0.5 per Share (Ordinary Dividend),5
Dividends,Data,Total in CHF,,,4.5
Withholding Tax,Header,Currency,Date,Description,Amount,Code
Withholding Tax,Data,USD,2025-04-01,VTI(US9229087690) Cash Dividend USD 0.5 per Share - US Tax,-0.75,
Interest,Header,Currency,Date,Description,Amount
Interest,Data,USD,2025-05-06,USD Debit Interest for Apr-2025,-0.73
Fees,Header,Subtitle,Currency,Date,Description,Amount
Fees,Data,Other Fees,CHF,2025-06-01,Withdrawal Fee,-11
Fees,Data,Total,,,,-11
Open Positions,Header,DataDiscriminator,Asset Category,Currency,Symbol,Quantity,Mult,Cost Price,Cost Basis,Close Price,Value,Unrealized P/L,Unrealized P/L %,Code
Open Positions,Data,Summary,Stocks,USD,VTI,10,1,250.123,2501.23,260,2600,98.77,3.95,
Open Positions,Total,,Stocks,USD,,,,,2501.23,,2600,98.77,,
Forex Balances,Header,Asset Category,Currency,Description,Quantity,Cost Price,Cost Basis in CHF,Close Price,Value in CHF,Unrealized P/L in CHF,Code
Forex Balances,Data,Forex,CHF,USD,-498.712,0.9,-448.84,0.8,-398.97,49.87,
Forex Balances,Data,Total,,,,,-448.84,,-398.97,49.87,
Cash Report,Header,Currency Summary,Currency,Total,Securities,Futures,
Cash Report,Data,Starting Cash,Base Currency Summary,0,0,0,
Cash Report,Data,Ending Cash,Base Currency Summary,2786.422,2786.422,0,
Cash Report,Data,Starting Cash,CHF,0,0,0,
Cash Report,Data,Ending Cash,CHF,3185.392,3185.392,0,
Cash Report,Data,Starting Cash,USD,0,0,0,
Cash Report,Data,Ending Cash,USD,-498.712,-498.712,0,
";
        let statement = Statement::parse(source).unwrap();
        assert_eq!(
            statement,
            Statement {
                base_currency: "CHF".into(),
                period_end: date("2025-12-31"),
                trades: vec![
                    Trade {
                        category: AssetCategory::Stocks,
                        date: date("2025-03-03"),
                        asset: "VTI".into(),
                        currency: "USD".into(),
                        quantity: dec("10"),
                        price: dec("250.123"),
                        proceeds: dec("-2501.23"),
                        fee: dec("-1"),
                    },
                    Trade {
                        category: AssetCategory::Forex,
                        date: date("2025-03-01"),
                        asset: "USD".into(),
                        currency: "CHF".into(),
                        quantity: dec("1000"),
                        price: dec("0.9"),
                        proceeds: dec("-900.00"),
                        fee: dec("-1.80"),
                    },
                    Trade {
                        category: AssetCategory::Forex,
                        date: date("2025-03-02"),
                        asset: "USD".into(),
                        currency: "CHF".into(),
                        quantity: dec("1000"),
                        price: dec("0.9"),
                        proceeds: dec("-900.00"),
                        fee: dec("-1.80"),
                    },
                ],
                transfers: vec![Cash {
                    date: date("2025-02-01"),
                    currency: "CHF".into(),
                    description: "Electronic Fund Transfer".into(),
                    amount: dec("5000"),
                }],
                dividends: vec![Cash {
                    date: date("2025-04-01"),
                    currency: "USD".into(),
                    description:
                        "VTI(US9229087690) Cash Dividend USD 0.5 per Share (Ordinary Dividend)"
                            .into(),
                    amount: dec("5"),
                }],
                withholding_taxes: vec![Cash {
                    date: date("2025-04-01"),
                    currency: "USD".into(),
                    description: "VTI(US9229087690) Cash Dividend USD 0.5 per Share - US Tax"
                        .into(),
                    amount: dec("-0.75"),
                }],
                interest: vec![Cash {
                    date: date("2025-05-06"),
                    currency: "USD".into(),
                    description: "USD Debit Interest for Apr-2025".into(),
                    amount: dec("-0.73"),
                }],
                fees: vec![Cash {
                    date: date("2025-06-01"),
                    currency: "CHF".into(),
                    description: "Withdrawal Fee".into(),
                    amount: dec("-11"),
                }],
                positions: vec![
                    Position {
                        commodity: "VTI".into(),
                        quantity: dec("10"),
                    },
                    Position {
                        commodity: "USD".into(),
                        quantity: dec("-498.71"),
                    },
                ],
                cash_balances: vec![
                    CashBalance {
                        currency: "CHF".into(),
                        start: dec("0"),
                        end: dec("3185.39"),
                    },
                    CashBalance {
                        currency: "USD".into(),
                        start: dec("0"),
                        end: dec("-498.71"),
                    },
                ],
            }
        );
    }

    #[test]
    fn test_parse_errors() {
        let err = |source: &str| Statement::parse(source).unwrap_err().to_string();
        assert_eq!(err(""), "no base currency found");
        assert_eq!(
            err("Account Information,Data,Base Currency,CHF\n"),
            "no statement period found"
        );
        assert!(err("Statement,Data,Period,yesterday\n").starts_with("invalid period"));
    }

    #[test]
    fn test_parse_decimal() {
        assert_eq!(parse_decimal("").unwrap(), Decimal::ZERO);
        assert_eq!(parse_decimal("1,234.5").unwrap(), dec("1234.5"));
        assert_eq!(parse_rounded("-900.005").unwrap(), dec("-900.01"));
        assert!(parse_decimal("x").is_err());
    }
}
