use crate::model::{
    Account, AccountType, Assertion, Close, Command, Commodity, Lot, Open, Posting, Price, Tag,
    Transaction,
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
        if let Directive::Command(c) = &self {
            write!(f, "{}", c)?;
        }
        Ok(())
    }
}

pub fn parse(s: &mut Scanner) -> Result<Vec<Directive>> {
    let mut result = Vec::new();
    while let Some(_) = s.current() {
        s.skip_while(|c| c.is_ascii_whitespace());
        if let Some(c) = s.current() {
            match c {
                '0'..='9' => {
                    let c = parse_command(s)?;
                    result.push(Directive::Command(c))
                }
                '*' | '#' => {
                    s.consume_rest_of_line()?;
                }
                'i' => {
                    parse_include(s)?;
                }
                _ => {
                    return Err(s.error(
                        Some("unexpected character".into()),
                        Character::Custom("directive or comment".into()),
                        Character::Char(c),
                    ))
                }
            };
        }
    }
    Ok(result)
}

pub fn parse_command(s: &mut Scanner) -> Result<Command> {
    s.mark_position();
    let d = parse_date(s)?.0;
    s.consume_space1()?;
    match s.current() {
        Some('p') => Ok(Command::Price(parse_price(d, s)?)),
        Some('"') => Ok(Command::Trx(parse_transaction(d, s)?)),
        Some('o') => Ok(Command::Open(parse_open(d, s)?)),
        Some('b') => Ok(Command::Assertion(parse_assertion(d, s)?)),
        Some('c') => Ok(Command::Close(parse_close(d, s)?)),
        c => Err(s.error(
            Some("error parsing directive".into()),
            Character::Custom("directive".into()),
            Character::from_char(c),
        )),
    }
}

fn parse_account_type(s: &mut Scanner) -> Result<Annotated<AccountType>> {
    s.mark_position();
    let str = s.read_identifier()?;
    match str.0 {
        "Assets" => s.annotate(AccountType::Assets),
        "Liabilities" => s.annotate(AccountType::Liabilities),
        "Equity" => s.annotate(AccountType::Equity),
        "Income" => s.annotate(AccountType::Income),
        "Expenses" => s.annotate(AccountType::Expenses),
        _ => Err(s.error(
            Some("error parsing account type".into()),
            Character::Either(vec![
                Character::Custom("Assets".into()),
                Character::Custom("Liabilities".into()),
                Character::Custom("Equity".into()),
                Character::Custom("Income".into()),
                Character::Custom("Expenses".into()),
            ]),
            Character::Custom(str.0.into()),
        )),
    }
}

fn parse_date(s: &mut Scanner) -> Result<Annotated<NaiveDate>> {
    s.mark_position();
    let b = s.read_n(10)?;
    match NaiveDate::parse_from_str(b.0, "%Y-%m-%d") {
        Ok(d) => s.annotate(d),
        Err(_) => Err(s.error(
            Some("error parsing date".into()),
            Character::Custom("date (YYYY-MM-DD)".into()),
            Character::Custom(b.0.into()),
        )),
    }
}

fn parse_account(s: &mut Scanner) -> Result<Annotated<Account>> {
    s.mark_position();
    let account_type = parse_account_type(s)?.0;
    let mut segments = Vec::new();
    while let Some(':') = s.current() {
        s.consume_char(':')?;
        match s.read_identifier() {
            Ok(Annotated(t, _)) => segments.push(t),
            Err(e) => {
                return Err(s.error(
                    Some("error parsing account".into()),
                    Character::Custom("account".into()),
                    Character::Custom(format!("{}", e)),
                ))
            }
        }
    }
    s.annotate(Account::new(account_type, segments))
}

fn parse_open(d: NaiveDate, s: &mut Scanner) -> Result<Open> {
    s.consume_string("open")?.0;
    s.consume_space1()?;
    let a = parse_account(s)?.0;
    s.consume_space1()?;
    s.consume_eol()?;
    Ok(Open {
        date: d,
        account: a,
    })
}

fn parse_close(d: NaiveDate, s: &mut Scanner) -> Result<Close> {
    s.consume_string("close")?.0;
    s.consume_space1()?;
    let a = parse_account(s)?.0;
    s.consume_space1()?;
    s.consume_eol()?;
    Ok(Close {
        date: d,
        account: a,
    })
}

fn parse_transaction(d: NaiveDate, s: &mut Scanner) -> Result<Transaction> {
    let desc = s.read_quoted_string()?.0;
    s.consume_space1()?;
    let tags = parse_tags(s)?;
    s.consume_eol()?;
    let (postings, account) = parse_postings(s, d)?;
    Transaction::new(d, desc.into(), tags, postings, account).map_err(|e| {
        s.error(
            Some("error parsing transaction".into()),
            Character::Custom("a transaction".into()),
            Character::Custom(e.to_string()),
        )
    })
}

fn parse_tags(s: &mut Scanner) -> Result<Vec<Tag>> {
    let mut v = Vec::new();
    while let Some('#') = s.current() {
        v.push(parse_tag(s)?);
        s.consume_space1()?.0
    }
    Ok(v)
}

fn parse_tag(s: &mut Scanner) -> Result<Tag> {
    s.consume_char('#')?;
    Ok(Tag::new(s.read_identifier()?.0.into()))
}

fn parse_decimal(s: &mut Scanner) -> Result<Decimal> {
    let t = s.read_until(|c| c.is_whitespace())?.0;
    t.parse::<Decimal>().map_err(|_| {
        s.error(
            Some("error parsing decimal".into()),
            Character::Custom("a decimal value".into()),
            Character::Custom(t.to_string()),
        )
    })
}

fn parse_commodity(s: &mut Scanner) -> Result<Commodity> {
    Ok(Commodity::new(s.read_identifier()?.0.into()))
}

fn parse_lot(s: &mut Scanner, d: NaiveDate) -> Result<Lot> {
    s.consume_char('{')?;
    s.consume_space1()?;
    let price = parse_decimal(s)?;
    s.consume_space1()?;
    let commodity = parse_commodity(s)?;
    let mut label = None;
    let mut date = d;
    s.consume_space();
    while let Some(',') = s.current() {
        s.consume_char(',')?;
        s.consume_space();
        match s.current() {
            Some('"') => {
                label = Some(s.read_quoted_string()?);
                s.consume_space();
            }
            Some(d) if d.is_ascii_digit() => {
                date = parse_date(s)?.0;
                s.consume_space();
            }
            c => {
                return Err(s.error(
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
    s.consume_char('}')?;
    Ok(Lot::new(
        price,
        commodity,
        date,
        label.map(|s| s.0.to_string()),
    ))
}

fn parse_postings(s: &mut Scanner, d: NaiveDate) -> Result<(Vec<Posting>, Option<Account>)> {
    let mut postings = Vec::new();
    let mut wildcard = None;
    while s
        .current()
        .map(|c| c.is_ascii_alphanumeric())
        .unwrap_or(false)
    {
        let mut lot = None;
        let mut tag = None;
        let account = parse_account(s)?.0;
        s.consume_space1()?;
        if s.current().map_or(true, |c| c == '\n') {
            if wildcard.is_none() {
                wildcard = Some(account);
                s.consume_eol()?;
                continue;
            }
            return Err(s.error(
                Some("duplicate wildcard".into()),
                Character::Custom("".into()),
                Character::Custom("".into()),
            ));
        }
        let amount = parse_decimal(s)?;
        s.consume_space1()?;
        let commodity = parse_commodity(s)?;
        s.consume_space1()?;
        if let Some('{') = s.current() {
            lot = Some(parse_lot(s, d)?);
            s.consume_space1()?;
        }
        if let Some('#') = s.current() {
            tag = Some(parse_tag(s)?);
            s.consume_space1()?;
        }
        postings.push(Posting {
            account,
            commodity,
            amount,
            lot,
            tag,
        });
        s.consume_eol()?;
    }
    Ok((postings, wildcard))
}

fn parse_price(d: NaiveDate, s: &mut Scanner) -> Result<Price> {
    s.consume_string("price")?;
    s.consume_space1()?;
    let source = parse_commodity(s)?;
    s.consume_space1()?;
    let price = parse_decimal(s)?;
    s.consume_space1()?;
    let target = parse_commodity(s)?;
    s.consume_space1()?;
    s.consume_eol()?;
    Ok(Price::new(d, price, target, source))
}

fn parse_assertion(d: NaiveDate, s: &mut Scanner) -> Result<Assertion> {
    s.consume_string("balance")?;
    s.consume_space1()?;
    let account = parse_account(s)?.0;
    s.consume_space1()?;
    let price = parse_decimal(s)?;
    s.consume_space1()?;
    let commodity = parse_commodity(s)?;
    s.consume_space1()?;
    s.consume_eol()?;
    Ok(Assertion::new(d, account, price, commodity))
}

fn parse_include(s: &mut Scanner) -> Result<Directive> {
    s.consume_string("include")?;
    s.consume_space1()?;
    let directive = s
        .read_quoted_string()
        .map(|a| a.0)
        .map(std::path::PathBuf::from)
        .map(Directive::Include)?;
    s.consume_space1()?;
    s.consume_eol()?;
    Ok(directive)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::Result;

    #[test]
    fn test_parse_account_type() -> Result<()> {
        let mut s = Scanner::new("Assets");
        assert_eq!(parse_account_type(&mut s)?.0, AccountType::Assets);
        Ok(())
    }

    #[test]
    fn test_parse_date() -> Result<()> {
        let tests = [
            ("0202-02-02", chrono::NaiveDate::from_ymd(202, 2, 2), ""),
            ("2020-09-15 ", chrono::NaiveDate::from_ymd(2020, 9, 15), " "),
        ];
        for (test, expected, remainder) in tests {
            let mut s = Scanner::new(test);
            assert_eq!(parse_date(&mut s)?.0, expected);
            assert_eq!(s.read_all()?.0, remainder)
        }
        Ok(())
    }

    #[test]
    fn test_parse_account() -> Result<()> {
        let tests = [
            ("Assets", Account::new(AccountType::Assets, Vec::new())),
            (
                "Liabilities:CreditCards:Visa",
                Account::new(
                    AccountType::Liabilities,
                    vec!["CreditCards".into(), "Visa".into()],
                ),
            ),
        ];
        for (test, expected) in tests {
            let mut s = Scanner::new(test);
            assert_eq!(parse_account(&mut s)?.0, expected);
        }
        Ok(())
    }

    #[test]
    fn test_parse_open() -> Result<()> {
        let tests = [(
            "open Assets:Account",
            NaiveDate::from_ymd(2020, 2, 2),
            Open {
                date: NaiveDate::from_ymd(2020, 2, 2),
                account: Account {
                    account_type: AccountType::Assets,
                    segments: vec!["Account".into()],
                },
            },
        )];
        for (test, d, want) in tests {
            let mut s = Scanner::new(test);
            assert_eq!(parse_open(d, &mut s)?, want)
        }
        Ok(())
    }

    #[test]
    fn test_parse_close() -> Result<()> {
        let tests = [(
            "close Assets:Account",
            NaiveDate::from_ymd(2020, 2, 2),
            Close {
                date: NaiveDate::from_ymd(2020, 2, 2),
                account: Account {
                    account_type: AccountType::Assets,
                    segments: vec!["Account".into()],
                },
            },
        )];
        for (test, d, want) in tests {
            let mut s = Scanner::new(test);
            assert_eq!(parse_close(d, &mut s)?, want)
        }
        Ok(())
    }

    #[test]
    fn test_parse_tags() -> Result<()> {
        let tests = [
            (
                "#tag1 #1tag   no more tags",
                vec![Tag::new("tag1".into()), Tag::new("1tag".into())],
                "no more tags",
            ),
            ("", vec![], ""),
        ];
        for (test, want, remainder) in tests {
            let mut s = Scanner::new(test);
            assert_eq!(parse_tags(&mut s)?, want);
            assert_eq!(s.read_all()?.0, remainder)
        }
        Ok(())
    }

    #[test]
    fn test_parse_decimal() -> Result<()> {
        let tests = [
            ("3.14", Decimal::new(314, 2), ""),
            ("-3.141", Decimal::new(-3141, 3), ""),
            ("3.14159265359", Decimal::new(314159265359, 11), ""),
        ];
        for (test, expected, remainder) in tests {
            let mut s = Scanner::new(test);
            assert_eq!(parse_decimal(&mut s)?, expected);
            assert_eq!(s.read_all()?.0, remainder)
        }
        Ok(())
    }

    #[test]
    fn test_parse_transaction() -> Result<()> {
        let posting = Posting {
            account: Account::new(AccountType::Assets, vec!["Account1".into()]),
            amount: Decimal::new(24545, 2),
            commodity: Commodity::new("CHF".into()),
            lot: None,
            tag: None,
        };
        let tests = [
            (
                "\"some description\"\nAssets:Account1 245.45 CHF\nIncome:Gains1 -245.45 CHF",
                NaiveDate::from_ymd(2020, 1, 30),
                Transaction::new(
                    NaiveDate::from_ymd(2020, 1, 30),
                    "some description".into(),
                    vec![],
                    vec![
                        posting.clone(),
                        Posting {
                            account: Account::new(AccountType::Income, vec!["Gains1".into()]),
                            amount: Decimal::new(-24545, 2),
                            ..(posting.clone())
                        },
                    ],
                    None,
                ).unwrap(),
            ),
            (
                "\"some description\" #tag1 #tag2 \nAssets:Account1 245.45 CHF\nIncome:Gains1 -245.45 CHF",
                NaiveDate::from_ymd(2020, 1, 30),
                Transaction::new(
                    NaiveDate::from_ymd(2020, 1, 30),
                    "some description".into(),
                    vec![Tag::new("tag1".into()), Tag::new("tag2".into())],
                    vec![
                        posting.clone(),
                        Posting {
                            account: Account::new(AccountType::Income, vec!["Gains1".into()]),
                            amount: Decimal::new(-24545, 2),
                            ..(posting.clone())
                        },
                    ],
                    None,
                ).unwrap(),
            ),
            (
                "\"some description\" #tag1 #tag2 \nAssets:Account1 245.45 CHF\nIncome:Gains1",
                NaiveDate::from_ymd(2020, 1, 30),
                Transaction::new(
                    NaiveDate::from_ymd(2020, 1, 30),
                    "some description".into(),
                    vec![Tag::new("tag1".into()), Tag::new("tag2".into())],
                    vec![
                        posting.clone(),
                        Posting {
                            account: Account::new(AccountType::Income, vec!["Gains1".into()]),
                            amount: Decimal::new(-24545, 2),
                            ..posting
                        },
                    ],
                    None,
                ).unwrap(),
            ),
        ];
        for (test, date, expected) in tests {
            let mut s = Scanner::new(test);
            assert_eq!(parse_transaction(date, &mut s)?, expected);
        }
        Ok(())
    }

    #[test]
    fn test_parse_postings() -> Result<()> {
        let posting = Posting {
            account: Account::new(AccountType::Assets, vec!["Account1".into()]),
            amount: Decimal::new(24545, 2),
            commodity: Commodity::new("CHF".into()),
            lot: None,
            tag: None,
        };
        let tests = [
            (
                "Assets:Account1 245.45 CHF\nIncome:Gains1 -245.45 CHF",
                (
                    vec![
                        posting.clone(),
                        Posting {
                            amount: Decimal::new(-24545, 2),
                            account: Account::new(AccountType::Income, vec!["Gains1".into()]),
                            ..(posting.clone())
                        },
                    ],
                    None,
                ),
            ),
            (
                "Assets:Account1 245.45 CHF\nIncome:Gains1",
                (
                    vec![posting],
                    Some(Account::new(AccountType::Income, vec!["Gains1".into()])),
                ),
            ),
        ];
        for (test, expected) in tests {
            let mut s = Scanner::new(test);
            assert_eq!(
                parse_postings(&mut s, NaiveDate::from_ymd(2020, 2, 2))?,
                expected
            );
        }
        Ok(())
    }

    #[test]
    fn test_parse_price() -> Result<()> {
        let tests = [
            (
                "price USD 0.901 CHF",
                NaiveDate::from_ymd(2020, 2, 2),
                Price::new(
                    NaiveDate::from_ymd(2020, 2, 2),
                    Decimal::new(901, 3),
                    Commodity::new("CHF".into()),
                    Commodity::new("USD".into()),
                ),
            ),
            (
                "price 1MDB 1000000000 USD",
                NaiveDate::from_ymd(2020, 2, 2),
                Price::new(
                    NaiveDate::from_ymd(2020, 2, 2),
                    Decimal::new(1000000000, 0),
                    Commodity::new("USD".into()),
                    Commodity::new("1MDB".into()),
                ),
            ),
        ];
        for (test, d, want) in tests {
            let mut s = Scanner::new(test);
            assert_eq!(parse_price(d, &mut s)?, want)
        }
        Ok(())
    }

    #[test]
    fn test_parse_assertion() -> Result<()> {
        let tests = [
            (
                "balance Assets:MyAccount 0.901 USD",
                NaiveDate::from_ymd(2020, 2, 2),
                Assertion::new(
                    NaiveDate::from_ymd(2020, 2, 2),
                    Account::new(AccountType::Assets, vec!["MyAccount".into()]),
                    Decimal::new(901, 3),
                    Commodity::new("USD".into()),
                ),
            ),
            (
                "balance Liabilities:123foo 100 1CT",
                NaiveDate::from_ymd(2020, 2, 2),
                Assertion::new(
                    NaiveDate::from_ymd(2020, 2, 2),
                    Account::new(AccountType::Liabilities, vec!["123foo".into()]),
                    Decimal::new(100, 0),
                    Commodity::new("1CT".into()),
                ),
            ),
        ];
        for (test, d, want) in tests {
            let mut s = Scanner::new(test);
            assert_eq!(parse_assertion(d, &mut s)?, want)
        }
        Ok(())
    }
}
