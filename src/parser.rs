use crate::model::{
    Account, AccountType, Accrual, Assertion, Close, Command, Commodity, Interval, Lot, Open,
    Period, Posting, Price, Tag, Transaction, Value,
};
use crate::scanner::{Annotated, Character, Result, Scanner};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use std::fmt;
use std::fmt::Display;
use std::path::PathBuf;

#[derive(Debug)]
pub enum Directive {
    Command(Command),
    Include(PathBuf),
}

impl Display for Directive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Directive::Command(c) => write!(f, "{}", c)?,
            Directive::Include(p) => write!(f, "include \"{}\"", p.display())?,
        }
        Ok(())
    }
}

pub struct Parser<'a> {
    scanner: Scanner<'a>,
}

impl<'a> Parser<'a> {
    pub fn new(s: &str) -> Parser {
        Parser {
            scanner: Scanner::new(s),
        }
    }

    pub fn new_from_file(s: &str, filename: Option<PathBuf>) -> Parser {
        Parser {
            scanner: Scanner::new_from_file(s, filename),
        }
    }

    pub fn parse(&mut self) -> Result<Vec<Directive>> {
        let mut result = Vec::new();
        while self.scanner.current().is_some() {
            self.scanner.skip_while(|c| c.is_ascii_whitespace());
            if let Some(c) = self.scanner.current() {
                match c {
                    '0'..='9' | '@' => {
                        let c = self.parse_command()?;
                        result.push(Directive::Command(c.0))
                    }
                    '*' | '#' => {
                        self.scanner.consume_rest_of_line()?;
                    }
                    'i' => {
                        result.push(parse_include(&mut self.scanner)?.0);
                    }
                    _ => {
                        let pos = self.scanner.pos();
                        return Err(self.scanner.error(
                            pos,
                            Some("unexpected character".into()),
                            Character::Custom("directive or comment".into()),
                            Character::Char(c),
                        ));
                    }
                };
            }
        }
        Ok(result)
    }

    pub fn parse_command(&mut self) -> Result<Annotated<Command>> {
        let pos = self.scanner.pos();
        let cmd;
        if let Some('@') = self.scanner.current() {
            let a = parse_accrual(&mut self.scanner)?.0;
            let d = parse_date(&mut self.scanner)?.0;
            self.scanner.consume_space1()?;
            cmd = Command::Trx(parse_transaction(&mut self.scanner, d, Some(a))?.0);
        } else {
            let d = parse_date(&mut self.scanner)?.0;
            self.scanner.consume_space1()?;
            cmd = match self.scanner.current() {
                Some('p') => Command::Price(parse_price(d, &mut self.scanner)?.0),
                Some('"') => Command::Trx(parse_transaction(&mut self.scanner, d, None)?.0),
                Some('o') => Command::Open(parse_open(d, &mut self.scanner)?.0),
                Some('b') => Command::Assertion(parse_assertion(d, &mut self.scanner)?.0),
                Some('c') => Command::Close(parse_close(d, &mut self.scanner)?.0),
                Some('v') => Command::Value(parse_value(d, &mut self.scanner)?.0),
                c => {
                    return Err(self.scanner.error(
                        pos,
                        Some("error parsing directive".into()),
                        Character::Either(vec![
                            Character::Custom("open".into()),
                            Character::Custom("close".into()),
                            Character::Custom("price".into()),
                            Character::Custom("balance".into()),
                            Character::Custom("value".into()),
                            Character::Custom("<description>".into()),
                        ]),
                        Character::from_char(c),
                    ))
                }
            };
        }
        self.scanner.annotate(pos, cmd)
    }
}

fn parse_accrual(scanr: &mut Scanner) -> Result<Annotated<Accrual>> {
    let pos = scanr.pos();
    scanr.consume_string("@accrue")?;
    scanr.consume_space1()?;
    let interval = parse_interval(scanr)?.0;
    scanr.consume_space1()?;
    let start = parse_date(scanr)?.0;
    scanr.consume_space1()?;
    let end = parse_date(scanr)?.0;
    scanr.consume_space1()?;
    let account = parse_account(scanr)?.0;
    scanr.consume_space1()?;
    scanr.consume_eol()?;
    scanr.annotate(
        pos,
        Accrual {
            period: Period { start, end },
            interval,
            account,
        },
    )
}

fn parse_interval(scanr: &mut Scanner) -> Result<Annotated<Interval>> {
    let pos = scanr.pos();
    let str = scanr.read_identifier()?.0;
    match str.to_lowercase().as_str() {
        "once" => scanr.annotate(pos, Interval::Once),
        "daily" => scanr.annotate(pos, Interval::Daily),
        "weekly" => scanr.annotate(pos, Interval::Weekly),
        "monthly" => scanr.annotate(pos, Interval::Monthly),
        "quarterly" => scanr.annotate(pos, Interval::Quarterly),
        "yearly" => scanr.annotate(pos, Interval::Yearly),
        _ => Err(scanr.error(
            pos,
            Some("error parsing interval".into()),
            Character::Either(vec![
                Character::Custom("once".into()),
                Character::Custom("daily".into()),
                Character::Custom("weekly".into()),
                Character::Custom("monthly".into()),
                Character::Custom("quarterly".into()),
                Character::Custom("yearly".into()),
            ]),
            Character::Custom(str.into()),
        )),
    }
}

fn parse_account_type(scanr: &mut Scanner) -> Result<Annotated<AccountType>> {
    let pos = scanr.pos();
    let str = scanr.read_identifier()?.0;
    match str {
        "Assets" => scanr.annotate(pos, AccountType::Assets),
        "Liabilities" => scanr.annotate(pos, AccountType::Liabilities),
        "Equity" => scanr.annotate(pos, AccountType::Equity),
        "Income" => scanr.annotate(pos, AccountType::Income),
        "Expenses" => scanr.annotate(pos, AccountType::Expenses),
        _ => Err(scanr.error(
            pos,
            Some("error parsing account type".into()),
            Character::Either(vec![
                Character::Custom("Assets".into()),
                Character::Custom("Liabilities".into()),
                Character::Custom("Equity".into()),
                Character::Custom("Income".into()),
                Character::Custom("Expenses".into()),
            ]),
            Character::Custom(str.into()),
        )),
    }
}

fn parse_date(scanr: &mut Scanner) -> Result<Annotated<NaiveDate>> {
    let pos = scanr.pos();
    let b = scanr.read_n(10)?;
    match NaiveDate::parse_from_str(b.0, "%Y-%m-%d") {
        Ok(d) => scanr.annotate(pos, d),
        Err(_) => Err(scanr.error(
            pos,
            Some("error parsing date".into()),
            Character::Custom("date (YYYY-MM-DD)".into()),
            Character::Custom(b.0.into()),
        )),
    }
}

fn parse_account(scanr: &mut Scanner) -> Result<Annotated<Account>> {
    let pos = scanr.pos();
    let account_type = parse_account_type(scanr)?.0;
    let mut segments = Vec::new();
    while let Some(':') = scanr.current() {
        scanr.consume_char(':')?;
        match scanr.read_identifier() {
            Ok(Annotated(t, _)) => segments.push(t),
            Err(e) => {
                return Err(scanr.error(
                    pos,
                    Some("error parsing account".into()),
                    Character::Custom("account".into()),
                    Character::Custom(format!("{}", e)),
                ))
            }
        }
    }
    scanr.annotate(pos, Account::new(account_type, &segments))
}

fn parse_open(d: NaiveDate, scanr: &mut Scanner) -> Result<Annotated<Open>> {
    let pos = scanr.pos();
    scanr.consume_string("open")?;
    scanr.consume_space1()?;
    let a = parse_account(scanr)?.0;
    scanr.consume_space1()?;
    scanr.consume_eol()?;
    scanr.annotate(
        pos,
        Open {
            date: d,
            account: a,
        },
    )
}

fn parse_close(d: NaiveDate, scanr: &mut Scanner) -> Result<Annotated<Close>> {
    let pos = scanr.pos();
    scanr.consume_string("close")?;
    scanr.consume_space1()?;
    let a = parse_account(scanr)?.0;
    scanr.consume_space1()?;
    scanr.consume_eol()?;
    scanr.annotate(
        pos,
        Close {
            date: d,
            account: a,
        },
    )
}

fn parse_value(d: NaiveDate, scanr: &mut Scanner) -> Result<Annotated<Value>> {
    let pos = scanr.pos();
    scanr.consume_string("value")?;
    scanr.consume_space1()?;
    let acc = parse_account(scanr)?.0;
    scanr.consume_space1()?;
    let amt = parse_decimal(scanr)?.0;
    scanr.consume_space1()?;
    let com = parse_commodity(scanr)?.0;
    scanr.consume_space1()?;
    scanr.consume_eol()?;
    scanr.annotate(pos, Value::new(d, acc, amt, com))
}

fn parse_transaction(
    s: &mut Scanner,
    d: NaiveDate,
    acc: Option<Accrual>,
) -> Result<Annotated<Transaction>> {
    let pos = s.pos();
    let desc = s.read_quoted_string()?.0;
    s.consume_space1()?;
    let tags = parse_tags(s)?.0;
    s.consume_eol()?;
    let postings = parse_postings(s)?.0;
    let t = Transaction::new(d, desc.into(), tags, postings, acc);
    s.annotate(pos, t)
}

fn parse_tags(scanr: &mut Scanner) -> Result<Annotated<Vec<Tag>>> {
    let pos = scanr.pos();
    let mut v = Vec::new();
    while let Some('#') = scanr.current() {
        v.push(parse_tag(scanr)?.0);
        scanr.consume_space1()?.0
    }
    scanr.annotate(pos, v)
}

fn parse_tag(scanr: &mut Scanner) -> Result<Annotated<Tag>> {
    let pos = scanr.pos();
    scanr.consume_char('#')?;
    let tag = scanr.read_identifier()?.0;
    scanr.annotate(pos, Tag::new(tag.into()))
}

fn parse_decimal(s: &mut Scanner) -> Result<Annotated<Decimal>> {
    let pos = s.pos();
    let t = s.read_until(|c| c.is_whitespace())?.0;
    match t.parse::<Decimal>() {
        Ok(d) => s.annotate(pos, d),
        Err(_) => Err(s.error(
            pos,
            Some("error parsing decimal".into()),
            Character::Custom("a decimal value".into()),
            Character::Custom(t.to_string()),
        )),
    }
}

fn parse_commodity(scanr: &mut Scanner) -> Result<Annotated<Commodity>> {
    let pos = scanr.pos();
    let c = Commodity::new(scanr.read_identifier()?.0.into());
    scanr.annotate(pos, c)
}

fn parse_lot(scanr: &mut Scanner) -> Result<Lot> {
    let pos = scanr.pos();
    scanr.consume_char('{')?;
    scanr.consume_space1()?;
    let price = parse_decimal(scanr)?.0;
    scanr.consume_space1()?;
    let commodity = parse_commodity(scanr)?.0;
    let mut label = None;
    let mut date = None;
    scanr.consume_space();
    while let Some(',') = scanr.current() {
        scanr.consume_char(',')?;
        scanr.consume_space();
        match scanr.current() {
            Some('"') => {
                label = Some(scanr.read_quoted_string()?);
                scanr.consume_space();
            }
            Some(d) if d.is_ascii_digit() => {
                date = Some(parse_date(scanr)?.0);
                scanr.consume_space();
            }
            c => {
                return Err(scanr.error(
                    pos,
                    Some("error parsing lot".into()),
                    Character::Either(vec![
                        Character::Custom("label".into()),
                        Character::Custom("date (YYYY-MM-DD)".into()),
                    ]),
                    Character::from_char(c),
                ))
            }
        }
    }
    scanr.consume_char('}')?;
    Ok(Lot::new(
        price,
        commodity,
        date,
        label.map(|s| s.0.to_string()),
    ))
}

fn parse_postings(scanr: &mut Scanner) -> Result<Annotated<Vec<Posting>>> {
    let pos = scanr.pos();
    let mut postings = Vec::new();
    postings.push(parse_posting(scanr)?.0);
    while scanr
        .current()
        .map(|c| c.is_ascii_alphanumeric())
        .unwrap_or(false)
    {
        postings.push(parse_posting(scanr)?.0)
    }
    scanr.annotate(pos, postings)
}

fn parse_posting(scanr: &mut Scanner) -> Result<Annotated<Posting>> {
    let pos = scanr.pos();
    let mut lot = None;
    let mut targets = None;
    let credit = parse_account(scanr)?.0;

    scanr.consume_space1()?;
    let debit = parse_account(scanr)?.0;
    scanr.consume_space1()?;
    let amount = parse_decimal(scanr)?.0;
    scanr.consume_space1()?;
    let commodity = parse_commodity(scanr)?.0;
    scanr.consume_space1()?;
    if let Some('(') = scanr.current() {
        targets = Some(parse_targets(scanr)?.0);
        scanr.consume_space1()?;
    }
    if let Some('{') = scanr.current() {
        lot = Some(parse_lot(scanr)?);
        scanr.consume_space1()?;
    }
    let posting = scanr.annotate(
        pos,
        Posting {
            credit,
            debit,
            commodity,
            amount,
            lot,
            targets,
        },
    );
    scanr.consume_eol()?;
    posting
}

fn parse_targets(scanr: &mut Scanner) -> Result<Annotated<Vec<Commodity>>> {
    let pos = scanr.pos();
    let mut targets = Vec::new();
    scanr.consume_char('(')?;
    scanr.consume_space();
    if Some(')') != scanr.current() {
        targets.push(parse_commodity(scanr)?.0);
        scanr.consume_space();
    }
    while let Some(',') = scanr.current() {
        scanr.consume_char(',')?;
        scanr.consume_space();
        targets.push(parse_commodity(scanr)?.0);
        scanr.consume_space();
    }
    scanr.consume_char(')')?;
    scanr.annotate(pos, targets)
}

fn parse_price(d: NaiveDate, scanr: &mut Scanner) -> Result<Annotated<Price>> {
    let pos = scanr.pos();
    scanr.consume_string("price")?;
    scanr.consume_space1()?;
    let source = parse_commodity(scanr)?.0;
    scanr.consume_space1()?;
    let price = parse_decimal(scanr)?.0;
    scanr.consume_space1()?;
    let target = parse_commodity(scanr)?.0;
    scanr.consume_space1()?;
    scanr.consume_eol()?;
    scanr.annotate(pos, Price::new(d, price, target, source))
}

fn parse_assertion(d: NaiveDate, scanr: &mut Scanner) -> Result<Annotated<Assertion>> {
    let pos = scanr.pos();
    scanr.consume_string("balance")?;
    scanr.consume_space1()?;
    let account = parse_account(scanr)?.0;
    scanr.consume_space1()?;
    let price = parse_decimal(scanr)?.0;
    scanr.consume_space1()?;
    let commodity = parse_commodity(scanr)?.0;
    scanr.consume_space1()?;
    scanr.consume_eol()?;
    scanr.annotate(pos, Assertion::new(d, account, price, commodity))
}

fn parse_include(scanr: &mut Scanner) -> Result<Annotated<Directive>> {
    let pos = scanr.pos();
    scanr.consume_string("include")?;
    scanr.consume_space1()?;
    let directive = scanr
        .read_quoted_string()
        .map(|a| a.0)
        .map(std::path::PathBuf::from)
        .map(Directive::Include)?;
    scanr.consume_space1()?;
    scanr.consume_eol()?;
    scanr.annotate(pos, directive)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_account_type() {
        assert_eq!(
            parse_account_type(&mut Scanner::new("Assets")).unwrap().0,
            AccountType::Assets
        );
    }

    #[test]
    fn test_parse_date() {
        assert_eq!(
            parse_date(&mut Scanner::new("0202-02-02")).unwrap(),
            Annotated(chrono::NaiveDate::from_ymd_opt(202, 2, 2).unwrap(), (0, 10))
        );
        assert_eq!(
            parse_date(&mut Scanner::new("2020-09-15")).unwrap(),
            Annotated(
                chrono::NaiveDate::from_ymd_opt(2020, 9, 15).unwrap(),
                (0, 10)
            )
        );
    }

    #[test]
    fn test_parse_account() {
        assert_eq!(
            parse_account(&mut Scanner::new("Assets")).unwrap(),
            Annotated(Account::new(AccountType::Assets, &[]), (0, 6))
        );
        assert_eq!(
            parse_account(&mut Scanner::new("Liabilities:CreditCards:Visa")).unwrap(),
            Annotated(
                Account::new(AccountType::Liabilities, &["CreditCards", "Visa"]),
                (0, 28)
            )
        );
    }

    #[test]
    fn test_parse_open() {
        assert_eq!(
            parse_open(
                NaiveDate::from_ymd_opt(2020, 2, 2).unwrap(),
                &mut Scanner::new("open Assets:Account")
            )
            .unwrap(),
            Annotated(
                Open {
                    date: NaiveDate::from_ymd_opt(2020, 2, 2).unwrap(),
                    account: Account {
                        account_type: AccountType::Assets,
                        segments: vec!["Account".into()],
                    },
                },
                (0, 19)
            )
        )
    }

    #[test]
    fn test_parse_close() {
        assert_eq!(
            parse_close(
                NaiveDate::from_ymd_opt(2020, 2, 2).unwrap(),
                &mut Scanner::new("close Assets:Account")
            )
            .unwrap(),
            Annotated(
                Close {
                    date: NaiveDate::from_ymd_opt(2020, 2, 2).unwrap(),
                    account: Account {
                        account_type: AccountType::Assets,
                        segments: vec!["Account".into()],
                    },
                },
                (0, 20)
            )
        )
    }

    #[test]
    fn test_parse_value() {
        assert_eq!(
            parse_value(
                NaiveDate::from_ymd_opt(2020, 2, 2).unwrap(),
                &mut Scanner::new("value  Assets:Account  101.40 KNUT")
            )
            .unwrap(),
            Annotated(
                Value::new(
                    NaiveDate::from_ymd_opt(2020, 2, 2).unwrap(),
                    Account::new(AccountType::Assets, &["Account".into()]),
                    Decimal::new(10140, 2),
                    Commodity::new("KNUT".into()),
                ),
                (0, 34)
            )
        )
    }

    #[test]
    fn test_parse_tags() {
        assert_eq!(
            parse_tags(&mut Scanner::new("#tag1 #1tag   no more tags")).unwrap(),
            Annotated(
                vec![Tag::new("tag1".into()), Tag::new("1tag".into())],
                (0, 14)
            )
        );
    }

    #[test]
    fn test_parse_decimal() {
        assert_eq!(
            parse_decimal(&mut Scanner::new("3.14")).unwrap(),
            Annotated(Decimal::new(314, 2), (0, 4))
        );
        assert_eq!(
            parse_decimal(&mut Scanner::new("-3.141")).unwrap(),
            Annotated(Decimal::new(-3141, 3), (0, 6))
        );
        assert_eq!(
            parse_decimal(&mut Scanner::new("3.14159265359")).unwrap(),
            Annotated(Decimal::new(314159265359, 11), (0, 13))
        );
    }

    #[test]
    fn test_parse_transaction() {
        let tests = [
            (
                "\"some description\"\nAssets:Account1 Expenses:Trading 245.45 CHF (ABC)\nIncome:Gains1 Assets:Foo -245.45 CHF",
                NaiveDate::from_ymd_opt(2020, 1, 30).unwrap(),
                Annotated(Transaction::new(
                    NaiveDate::from_ymd_opt(2020, 1, 30).unwrap(),
                    "some description".into(),
                    vec![],
                    vec![
                        Posting {
                            credit: Account::new(AccountType::Assets, &["Account1"]),
                            debit: Account::new(AccountType::Expenses, &["Trading"]),
                            amount: Decimal::new(24545, 2),
                            commodity: Commodity::new("CHF".into()),
                            lot: None,
                            targets: Some(vec![Commodity::new("ABC".into())]),
                        },
                        Posting {
                            credit: Account::new(AccountType::Income, &["Gains1"]),
                            debit: Account::new(AccountType::Assets, &["Foo"]),
                            amount: Decimal::new(-24545, 2),
                            commodity:             Commodity::new("CHF".into()),
                            lot: None,
                            targets:None,
                        },
                    ],
                    None,
                ), (0, 105)),
            ),
            (
                "\"some description\" #tag1 #tag2 \nAssets:Account1 Assets:Account2   245.45 CHF\nIncome:Gains Assets:Account2 10000 USD",
                NaiveDate::from_ymd_opt(2020, 1, 30).unwrap(),
                Annotated(Transaction::new(
                    NaiveDate::from_ymd_opt(2020, 1, 30).unwrap(),
                    "some description".into(),
                    vec![Tag::new("tag1".into()), Tag::new("tag2".into())],
                    vec![
                        Posting {
                            credit: Account::new(AccountType::Assets, &["Account1"]),
                            debit: Account::new(AccountType::Assets, &["Account2"]),
                            amount: Decimal::new(24_545, 2),
                            commodity: Commodity::new("CHF".into()),
                            lot: None,
                            targets:None,
                         },
                         Posting {
                            credit: Account::new(AccountType::Income, &["Gains"]),
                            debit: Account::new(AccountType::Assets, &["Account2"]),
                            amount: Decimal::new(1_000_000, 2),
                            commodity: Commodity::new("USD".into()),
                            lot: None,
                            targets:None,
                         },
                    ],None), (0, 115)),
            ),
        ];
        for (test, date, want) in tests {
            let mut s = Scanner::new(test);
            assert_eq!(parse_transaction(&mut s, date, None).unwrap(), want);
        }
    }

    #[test]
    fn test_parse_accrual() {
        assert_eq!(
            parse_accrual(&mut Scanner::new(
                "@accrue daily 2022-04-03 2022-05-05 Liabilities:Accruals"
            ))
            .unwrap(),
            Annotated(
                Accrual {
                    account: Account::new(AccountType::Liabilities, &["Accruals"]),
                    interval: Interval::Daily,
                    period: Period {
                        start: NaiveDate::from_ymd_opt(2022, 4, 3).unwrap(),
                        end: NaiveDate::from_ymd_opt(2022, 5, 5).unwrap(),
                    },
                },
                (0, 56),
            ),
        );
        assert_eq!(
            parse_accrual(&mut Scanner::new(
                "@accrue monthly 2022-04-03     2022-05-05     Liabilities:Bank    "
            ))
            .unwrap(),
            Annotated(
                Accrual {
                    account: Account::new(AccountType::Liabilities, &["Bank"]),
                    interval: Interval::Monthly,
                    period: Period {
                        start: NaiveDate::from_ymd_opt(2022, 4, 3).unwrap(),
                        end: NaiveDate::from_ymd_opt(2022, 5, 5).unwrap(),
                    },
                },
                (0, 66),
            ),
        );
    }

    #[test]
    fn test_parse_interval() {
        assert_eq!(
            parse_interval(&mut Scanner::new("daily")).unwrap(),
            Annotated(Interval::Daily, (0, 5))
        );
        assert_eq!(
            parse_interval(&mut Scanner::new("weekly")).unwrap(),
            Annotated(Interval::Weekly, (0, 6))
        );
        assert_eq!(
            parse_interval(&mut Scanner::new("monthly")).unwrap(),
            Annotated(Interval::Monthly, (0, 7))
        );
        assert_eq!(
            parse_interval(&mut Scanner::new("quarterly")).unwrap(),
            Annotated(Interval::Quarterly, (0, 9))
        );
        assert_eq!(
            parse_interval(&mut Scanner::new("yearly")).unwrap(),
            Annotated(Interval::Yearly, (0, 6))
        );
        assert_eq!(
            parse_interval(&mut Scanner::new("once")).unwrap(),
            Annotated(Interval::Once, (0, 4))
        );
    }

    #[test]
    fn test_parse_postings() {
        assert_eq!(
            parse_postings(
                &mut Scanner::new("Assets:Account1    Assets:Account2   4.00    CHF\nAssets:Account2    Assets:Account1   3.00 USD")).unwrap(),
            Annotated(
                vec![
                    Posting {
                        credit: Account::new(AccountType::Assets, &["Account1"]),
                        debit: Account::new(AccountType::Assets, &["Account2"]),
                        amount: Decimal::new(400, 2),
                        commodity: Commodity::new("CHF".into()),
                        lot: None,
                        targets: None,
                    },
                    Posting {
                        credit: Account::new(AccountType::Assets, &["Account2"]),
                        debit: Account::new(AccountType::Assets, &["Account1"]),
                        amount: Decimal::new(300, 2),
                        commodity: Commodity::new("USD".into()),
                        lot: None,
                        targets: None,
                    },
                ],
                (0, 94),
            )
        );
    }

    #[test]
    fn test_parse_targets() {
        assert_eq!(
            parse_targets(&mut Scanner::new("(A,B,  C   )")).unwrap(),
            Annotated(
                vec![
                    Commodity::new("A".into()),
                    Commodity::new("B".into()),
                    Commodity::new("C".into()),
                ],
                (0, 12),
            )
        );
    }

    #[test]
    fn test_parse_posting() {
        assert_eq!(
            parse_posting(&mut Scanner::new(
                "Assets:Account1    Assets:Account2   4.00    CHF"
            ))
            .unwrap(),
            Annotated(
                Posting {
                    credit: Account::new(AccountType::Assets, &["Account1"]),
                    debit: Account::new(AccountType::Assets, &["Account2"]),
                    amount: Decimal::new(400, 2),
                    commodity: Commodity::new("CHF".into()),
                    lot: None,
                    targets: None,
                },
                (0, 48),
            )
        );
    }

    #[test]
    fn test_parse_price() {
        let date = NaiveDate::from_ymd_opt(2020, 2, 2).unwrap();
        assert_eq!(
            parse_price(date, &mut Scanner::new("price USD 0.901 CHF")).unwrap(),
            Annotated(
                Price::new(
                    NaiveDate::from_ymd_opt(2020, 2, 2).unwrap(),
                    Decimal::new(901, 3),
                    Commodity::new("CHF".into()),
                    Commodity::new("USD".into()),
                ),
                (0, 19),
            )
        );
        assert_eq!(
            parse_price(date, &mut Scanner::new("price 1MDB 1000000000 USD")).unwrap(),
            Annotated(
                Price::new(
                    NaiveDate::from_ymd_opt(2020, 2, 2).unwrap(),
                    Decimal::new(1000000000, 0),
                    Commodity::new("USD".into()),
                    Commodity::new("1MDB".into()),
                ),
                (0, 25),
            )
        )
    }

    #[test]
    fn test_parse_assertion() {
        let d = NaiveDate::from_ymd_opt(2020, 2, 2).unwrap();
        assert_eq!(
            parse_assertion(d, &mut Scanner::new("balance Assets:MyAccount 0.901 USD")).unwrap(),
            Annotated(
                Assertion::new(
                    NaiveDate::from_ymd_opt(2020, 2, 2).unwrap(),
                    Account::new(AccountType::Assets, &["MyAccount"]),
                    Decimal::new(901, 3),
                    Commodity::new("USD".into()),
                ),
                (0, 34),
            )
        );
        assert_eq!(
            parse_assertion(d, &mut Scanner::new("balance Liabilities:123foo 100 1CT")).unwrap(),
            Annotated(
                Assertion::new(
                    NaiveDate::from_ymd_opt(2020, 2, 2).unwrap(),
                    Account::new(AccountType::Liabilities, &["123foo"]),
                    Decimal::new(100, 0),
                    Commodity::new("1CT".into()),
                ),
                (0, 34),
            )
        );
    }
}
