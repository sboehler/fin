use crate::model::{
    Account, AccountType, Assertion, Close, Command, Commodity, Lot, Open, Posting, Price, Tag,
    Transaction,
};
use crate::scanner::{
    consume_char, consume_eol, consume_rest_of_line, consume_space, consume_space1, consume_string,
    consume_while, read_identifier, read_quoted_string, read_string, read_while, Scanner,
};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use std::io::{Error, ErrorKind, Read, Result};
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Debug)]
pub enum Directive {
    Command(Command),
    Include(PathBuf),
}

pub fn parse<R: Read>(s: &mut Scanner<R>) -> Result<Vec<Directive>> {
    let mut result = Vec::new();
    while s.current().is_some() {
        consume_while(s, |c| c.is_ascii_whitespace())?;
        if let Some(c) = s.current() {
            match c {
                '0'..='9' => {
                    let c = parse_command(s)?;
                    print!("{:#?}", c);
                    result.push(Directive::Command(c))
                }
                '*' | '#' => {
                    consume_rest_of_line(s)?;
                }
                'i' => {
                    parse_include(s)?;
                }
                _ => {
                    return Err(Error::new(
                        ErrorKind::InvalidData,
                        format!("Expected a directive, got {}", c),
                    ))
                }
            };
        }
    }
    Ok(result)
}

pub fn parse_command<R: Read>(s: &mut Scanner<R>) -> Result<Command> {
    let d = parse_date(s)?;
    consume_space1(s)?;
    match s.current() {
        Some('p') => Ok(Command::Price(parse_price(d, s)?)),
        Some('"') => Ok(Command::Trx(parse_transaction(d, s)?)),
        Some('o') => Ok(Command::Open(parse_open(d, s)?)),
        Some('b') => Ok(Command::Assertion(parse_assertion(d, s)?)),
        Some('c') => Ok(Command::Close(parse_close(d, s)?)),
        Some(c) => Err(Error::new(
            ErrorKind::InvalidData,
            format!("Expected directive, found '{}'", c),
        )),
        None => Err(Error::new(
            ErrorKind::UnexpectedEof,
            format!("Expected directive, found EOF"),
        )),
    }
}

fn parse_account_type<R: Read>(s: &mut Scanner<R>) -> Result<AccountType> {
    let s = read_identifier(s)?;
    match s.as_str() {
        "Assets" => Ok(AccountType::Assets),
        "Liabilities" => Ok(AccountType::Liabilities),
        "Equity" => Ok(AccountType::Equity),
        "Income" => Ok(AccountType::Income),
        "Expenses" => Ok(AccountType::Expenses),
        "TBD" => Ok(AccountType::TBD),
        _ => Err(Error::new(
            ErrorKind::InvalidData,
            format!("Expected account type, got '{}'", s),
        )),
    }
}

fn parse_date<R: Read>(s: &mut Scanner<R>) -> Result<NaiveDate> {
    let b = read_string(s, 10)?;
    match NaiveDate::parse_from_str(b.as_str(), "%Y-%m-%d") {
        Ok(d) => Ok(d),
        Err(_) => Err(Error::new(
            ErrorKind::InvalidData,
            format!("Invalid date '{}'", b),
        )),
    }
}

fn parse_account<R: Read>(s: &mut Scanner<R>) -> Result<Account> {
    let account_type = parse_account_type(s)?;
    let mut segments = Vec::new();
    while let Some(':') = s.current() {
        consume_char(s, ':')?;
        segments.push(read_identifier(s)?)
    }
    Ok(Account::new(account_type, segments))
}

fn parse_open<R: Read>(d: NaiveDate, s: &mut Scanner<R>) -> Result<Open> {
    consume_string(s, "open")?;
    consume_space1(s)?;
    let a = parse_account(s)?;
    consume_space1(s)?;
    consume_eol(s)?;
    Ok(Open {
        date: d,
        account: a,
    })
}

fn parse_close<R: Read>(d: NaiveDate, s: &mut Scanner<R>) -> Result<Close> {
    consume_string(s, "close")?;
    consume_space1(s)?;
    let a = parse_account(s)?;
    consume_space1(s)?;
    consume_eol(s)?;
    Ok(Close {
        date: d,
        account: a,
    })
}

fn parse_transaction<R: Read>(d: NaiveDate, s: &mut Scanner<R>) -> Result<Transaction> {
    let desc = read_quoted_string(s)?;
    consume_space1(s)?;
    let tags = parse_tags(s)?;
    consume_eol(s)?;
    let (postings, account) = parse_postings(s, d)?;
    Transaction::new(d, desc, tags, postings, account)
}

fn parse_tags<R: Read>(s: &mut Scanner<R>) -> Result<Vec<Tag>> {
    let mut v = Vec::new();
    while let Some('#') = s.current() {
        v.push(parse_tag(s)?);
        consume_space1(s)?
    }
    Ok(v)
}

fn parse_tag<R: Read>(s: &mut Scanner<R>) -> Result<Tag> {
    consume_char(s, '#')?;
    Ok(Tag::new(read_identifier(s)?))
}

fn parse_decimal<R: Read>(s: &mut Scanner<R>) -> Result<Decimal> {
    let t = read_while(s, |c| *c == '-' || *c == '.' || c.is_ascii_digit())?;
    Decimal::from_str(&t).map_err(|e| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Error parsing decimal: {}", e),
        )
    })
}

fn parse_commodity<R: Read>(s: &mut Scanner<R>) -> Result<Commodity> {
    Ok(Commodity::new(read_identifier(s)?))
}

fn parse_lot<R: Read>(s: &mut Scanner<R>, d: NaiveDate) -> Result<Lot> {
    consume_char(s, '{')?;
    consume_space1(s)?;
    let price = parse_decimal(s)?;
    consume_space1(s)?;
    let commodity = parse_commodity(s)?;
    let mut label = None;
    let mut date = d;
    consume_space(s)?;
    while let Some(',') = s.current() {
        consume_char(s, ',')?;
        consume_space(s)?;
        match s.current() {
            Some('"') => {
                label = Some(read_quoted_string(s)?);
                consume_space(s)?;
            }
            Some(d) if d.is_ascii_digit() => {
                date = parse_date(s)?;
                consume_space(s)?;
            }
            Some(c) => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Expected label or date, got {}", c),
                ))
            }
            None => {
                return Err(Error::new(
                    ErrorKind::UnexpectedEof,
                    format!("Expected label or date, got EOF"),
                ))
            }
        }
    }
    consume_char(s, '}')?;
    Ok(Lot::new(price, commodity, date, label))
}

fn parse_postings<R: Read>(
    s: &mut Scanner<R>,
    d: NaiveDate,
) -> Result<(Vec<Posting>, Option<Account>)> {
    let mut postings = Vec::new();
    let mut wildcard = None;
    while s
        .current()
        .map(|c| c.is_ascii_alphanumeric())
        .unwrap_or(false)
    {
        let mut lot = None;
        let mut tag = None;
        let account = parse_account(s)?;
        consume_space1(s)?;
        if s.current().map_or(true, |c| c == '\n') {
            if wildcard.is_none() {
                wildcard = Some(account);
                consume_eol(s)?;
                continue;
            }
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Duplicate wildcard"),
            ));
        }
        let amount = parse_decimal(s)?;
        consume_space1(s)?;
        let commodity = parse_commodity(s)?;
        consume_space1(s)?;
        if let Some('{') = s.current() {
            lot = Some(parse_lot(s, d)?);
            consume_space1(s)?;
        }
        if let Some('#') = s.current() {
            tag = Some(parse_tag(s)?);
            consume_space1(s)?;
        }
        postings.push(Posting {
            account,
            commodity,
            amount,
            lot,
            tag,
        });
        consume_eol(s)?
    }
    Ok((postings, wildcard))
}

fn parse_price<R: Read>(d: NaiveDate, s: &mut Scanner<R>) -> Result<Price> {
    consume_string(s, "price")?;
    consume_space1(s)?;
    let source = parse_commodity(s)?;
    consume_space1(s)?;
    let price = parse_decimal(s)?;
    consume_space1(s)?;
    let target = parse_commodity(s)?;
    consume_space1(s)?;
    consume_eol(s)?;
    Ok(Price::new(d, price, target, source))
}

fn parse_assertion<R: Read>(d: NaiveDate, s: &mut Scanner<R>) -> Result<Assertion> {
    consume_string(s, "balance")?;
    consume_space1(s)?;
    let account = parse_account(s)?;
    consume_space1(s)?;
    let price = parse_decimal(s)?;
    consume_space1(s)?;
    let commodity = parse_commodity(s)?;
    consume_space1(s)?;
    consume_eol(s)?;
    Ok(Assertion::new(d, account, price, commodity))
}

fn parse_include<R: Read>(s: &mut Scanner<R>) -> Result<Directive> {
    consume_string(s, "include")?;
    consume_space1(s)?;
    let directive = read_quoted_string(s)
        .map(std::path::PathBuf::from)
        .map(Directive::Include)?;
    consume_space1(s)?;
    consume_eol(s)?;
    Ok(directive)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::read_all;
    use std::io::Result;

    #[test]
    fn test_parse_account_type() -> Result<()> {
        let mut s = Scanner::new("Assets".as_bytes());
        s.advance()?;
        assert_eq!(parse_account_type(&mut s)?, AccountType::Assets);
        Ok(())
    }

    #[test]
    fn test_parse_date() -> Result<()> {
        let tests = [
            ("0202-02-02", chrono::NaiveDate::from_ymd(202, 2, 2), ""),
            ("2020-09-15 ", chrono::NaiveDate::from_ymd(2020, 9, 15), " "),
        ];
        for (test, expected, remainder) in tests.iter() {
            let mut s = Scanner::new(test.as_bytes());
            s.advance()?;
            assert_eq!(parse_date(&mut s)?, *expected);
            assert_eq!(read_all(&mut s)?, *remainder)
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
        for (test, expected) in tests.iter() {
            let mut s = Scanner::new(test.as_bytes());
            s.advance()?;
            assert_eq!(parse_account(&mut s)?, *expected);
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
        for (test, d, want) in tests.iter() {
            let mut s = Scanner::new(test.as_bytes());
            s.advance()?;
            assert_eq!(parse_open(*d, &mut s)?, *want)
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
        for (test, d, want) in tests.iter() {
            let mut s = Scanner::new(test.as_bytes());
            s.advance()?;
            assert_eq!(parse_close(*d, &mut s)?, *want)
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
            ("".into(), vec![], "".into()),
        ];
        for (test, want, remainder) in tests.iter() {
            let mut s = Scanner::new(test.as_bytes());
            s.advance()?;
            assert_eq!(parse_tags(&mut s)?, *want);
            assert_eq!(read_all(&mut s)?, *remainder)
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
        for (test, expected, remainder) in tests.iter() {
            let mut s = Scanner::new(test.as_bytes());
            s.advance()?;
            assert_eq!(parse_decimal(&mut s)?, *expected);
            assert_eq!(read_all(&mut s)?, *remainder)
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
                )?,
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
                )?,
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
                            ..(posting.clone())
                        },
                    ],
                    None,
                )?,
            ),
        ];
        for (test, date, expected) in tests.iter() {
            let mut s = Scanner::new(test.as_bytes());
            s.advance()?;
            assert_eq!(parse_transaction(*date, &mut s)?, *expected);
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
                    vec![posting.clone()],
                    Some(Account::new(AccountType::Income, vec!["Gains1".into()])),
                ),
            ),
        ];
        for (test, expected) in tests.iter() {
            let mut s = Scanner::new(test.as_bytes());
            s.advance()?;
            assert_eq!(
                parse_postings(&mut s, NaiveDate::from_ymd(2020, 2, 2))?,
                *expected
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
        for (test, d, want) in tests.iter() {
            let mut s = Scanner::new(test.as_bytes());
            s.advance()?;
            assert_eq!(parse_price(*d, &mut s)?, *want)
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
        for (test, d, want) in tests.iter() {
            let mut s = Scanner::new(test.as_bytes());
            s.advance()?;
            assert_eq!(parse_assertion(*d, &mut s)?, *want)
        }
        Ok(())
    }
}
