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
        Some('"') => Ok(Command::Trx(parse_transaction(d, s)?.0)),
        Some('o') => Ok(Command::Open(parse_open(d, s)?.0)),
        Some('b') => Ok(Command::Assertion(parse_assertion(d, s)?)),
        Some('c') => Ok(Command::Close(parse_close(d, s)?.0)),
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

fn parse_open(d: NaiveDate, s: &mut Scanner) -> Result<Annotated<Open>> {
    s.mark_position();
    s.consume_string("open")?.0;
    s.consume_space1()?;
    let a = parse_account(s)?.0;
    s.consume_space1()?;
    s.consume_eol()?;
    s.annotate(Open {
        date: d,
        account: a,
    })
}

fn parse_close(d: NaiveDate, s: &mut Scanner) -> Result<Annotated<Close>> {
    s.mark_position();
    s.consume_string("close")?.0;
    s.consume_space1()?;
    let a = parse_account(s)?.0;
    s.consume_space1()?;
    s.consume_eol()?;
    s.annotate(Close {
        date: d,
        account: a,
    })
}

fn parse_transaction(d: NaiveDate, s: &mut Scanner) -> Result<Annotated<Transaction>> {
    s.mark_position();
    let desc = s.read_quoted_string()?.0;
    s.consume_space1()?;
    let tags = parse_tags(s)?.0;
    s.consume_eol()?;
    let postings = parse_postings(s)?.0;
    let t = Transaction::new(d, desc.into(), tags, postings);
    s.annotate(t)
}

fn parse_tags(s: &mut Scanner) -> Result<Annotated<Vec<Tag>>> {
    s.mark_position();
    let mut v = Vec::new();
    while let Some('#') = s.current() {
        v.push(parse_tag(s)?.0);
        s.consume_space1()?.0
    }
    s.annotate(v)
}

fn parse_tag(s: &mut Scanner) -> Result<Annotated<Tag>> {
    s.mark_position();
    s.consume_char('#')?;
    let tag = s.read_identifier()?.0;
    s.annotate(Tag::new(tag.into()))
}

fn parse_decimal(s: &mut Scanner) -> Result<Annotated<Decimal>> {
    s.mark_position();
    let t = s.read_until(|c| c.is_whitespace())?.0;
    match t.parse::<Decimal>() {
        Ok(d) => s.annotate(d),
        Err(_) => Err(s.error(
            Some("error parsing decimal".into()),
            Character::Custom("a decimal value".into()),
            Character::Custom(t.to_string()),
        )),
    }
}

fn parse_commodity(s: &mut Scanner) -> Result<Annotated<Commodity>> {
    s.mark_position();
    let c = Commodity::new(s.read_identifier()?.0.into());
    s.annotate(c)
}

fn parse_lot(s: &mut Scanner) -> Result<Lot> {
    s.consume_char('{')?;
    s.consume_space1()?;
    let price = parse_decimal(s)?.0;
    s.consume_space1()?;
    let commodity = parse_commodity(s)?.0;
    let mut label = None;
    let mut date = None;
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
                date = Some(parse_date(s)?.0);
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

fn parse_postings(s: &mut Scanner) -> Result<Annotated<Vec<Posting>>> {
    s.mark_position();
    let mut postings = Vec::new();
    while s
        .current()
        .map(|c| c.is_ascii_alphanumeric())
        .unwrap_or(false)
    {
        postings.push(parse_posting(s)?.0)
    }
    s.annotate(postings)
}

fn parse_posting(s: &mut Scanner) -> Result<Annotated<Posting>> {
    s.mark_position();
    let mut lot = None;
    let mut targets = None;
    let credit = parse_account(s)?.0;

    s.consume_space1()?;
    let debit = parse_account(s)?.0;
    s.consume_space1()?;
    let amount = parse_decimal(s)?.0;
    s.consume_space1()?;
    let commodity = parse_commodity(s)?.0;
    s.consume_space1()?;
    if let Some('{') = s.current() {
        lot = Some(parse_lot(s)?);
        s.consume_space1()?;
    }
    if let Some('(') = s.current() {
        targets = Some(parse_targets(s)?.0);
    }
    let posting = s.annotate(Posting {
        credit,
        debit,
        commodity,
        amount,
        lot,
        targets,
    });
    s.consume_eol()?;
    posting
}

fn parse_targets(s: &mut Scanner) -> Result<Annotated<Vec<Commodity>>> {
    s.mark_position();
    let mut targets = Vec::new();
    s.consume_char('(')?;
    loop {
        s.consume_space();
        targets.push(parse_commodity(s)?.0);
        s.consume_space();
        match s.current() {
            Some(',') => s.consume_char(',')?.0,
            Some(')') => {
                s.consume_char(')')?;
                return s.annotate(targets);
            }
            c => {
                return Err(s.error(
                    Some("error parsing target commodities".into()),
                    Character::Either(vec![Character::Char(')'), Character::Char(',')]),
                    Character::from_char(c),
                ))
            }
        }
    }
}

fn parse_price(d: NaiveDate, s: &mut Scanner) -> Result<Price> {
    s.consume_string("price")?;
    s.consume_space1()?;
    let source = parse_commodity(s)?.0;
    s.consume_space1()?.0;
    let price = parse_decimal(s)?.0;
    s.consume_space1()?;
    let target = parse_commodity(s)?.0;
    s.consume_space1()?.0;
    s.consume_eol()?;
    Ok(Price::new(d, price, target, source))
}

fn parse_assertion(d: NaiveDate, s: &mut Scanner) -> Result<Assertion> {
    s.consume_string("balance")?;
    s.consume_space1()?;
    let account = parse_account(s)?.0;
    s.consume_space1()?;
    let price = parse_decimal(s)?.0;
    s.consume_space1()?;
    let commodity = parse_commodity(s)?.0;
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

    #[test]
    fn test_parse_account_type() {
        let mut s = Scanner::new("Assets");
        assert_eq!(parse_account_type(&mut s).unwrap().0, AccountType::Assets);
    }

    #[test]
    fn test_parse_date() {
        let tests = [
            ("0202-02-02", chrono::NaiveDate::from_ymd(202, 2, 2), ""),
            ("2020-09-15 ", chrono::NaiveDate::from_ymd(2020, 9, 15), " "),
        ];
        for (test, want, remainder) in tests {
            let mut s = Scanner::new(test);
            assert_eq!(parse_date(&mut s).unwrap().0, want);
            assert_eq!(s.read_all().unwrap().0, remainder)
        }
    }

    #[test]
    fn test_parse_account() {
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
        for (test, want) in tests {
            let mut s = Scanner::new(test);
            assert_eq!(parse_account(&mut s).unwrap().0, want);
        }
    }

    #[test]
    fn test_parse_open() {
        let mut s = Scanner::new("open Assets:Account");
        assert_eq!(
            parse_open(NaiveDate::from_ymd(2020, 2, 2), &mut s).unwrap(),
            Annotated(
                Open {
                    date: NaiveDate::from_ymd(2020, 2, 2),
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
        let mut s = Scanner::new("close Assets:Account");
        assert_eq!(
            parse_close(NaiveDate::from_ymd(2020, 2, 2), &mut s).unwrap(),
            Annotated(
                Close {
                    date: NaiveDate::from_ymd(2020, 2, 2),
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
    fn test_parse_tags() {
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
            assert_eq!(parse_tags(&mut s).unwrap().0, want);
            assert_eq!(s.read_all().unwrap().0, remainder)
        }
    }

    #[test]
    fn test_parse_decimal() {
        let tests = [
            ("3.14", Decimal::new(314, 2), ""),
            ("-3.141", Decimal::new(-3141, 3), ""),
            ("3.14159265359", Decimal::new(314159265359, 11), ""),
        ];
        for (test, want, remainder) in tests {
            let mut s = Scanner::new(test);
            assert_eq!(
                parse_decimal(&mut s).unwrap(),
                Annotated(want, (0, test.len()))
            );
            assert_eq!(s.read_all().unwrap().0, remainder)
        }
    }

    #[test]
    fn test_parse_transaction() {
        let tests = [
            (
                "\"some description\"\nAssets:Account1 Expenses:Trading 245.45 CHF (ABC)\nIncome:Gains1 Assets:Foo -245.45 CHF",
                NaiveDate::from_ymd(2020, 1, 30),
                Annotated(Transaction::new(
                    NaiveDate::from_ymd(2020, 1, 30),
                    "some description".into(),
                    vec![],
                    vec![
                        Posting {
                            credit: Account::new(AccountType::Assets, vec!["Account1".into()]),
                            debit: Account::new(AccountType::Expenses, vec!["Trading".into()]),
                            amount: Decimal::new(24545, 2),
                            commodity: Commodity::new("CHF".into()),
                            lot: None,
                            targets: Some(vec![Commodity::new("ABC".into())]),
                        },
                        Posting {
                            credit: Account::new(AccountType::Income, vec!["Gains1".into()]),
                            debit: Account::new(AccountType::Assets, vec!["Foo".into()]),
                            amount: Decimal::new(-24545, 2),
                            commodity:             Commodity::new("CHF".into()),
                            lot: None,
                            targets:None,
                        },
                    ],
                ), (0, 105)),
            ),
            (
                "\"some description\" #tag1 #tag2 \nAssets:Account1 Assets:Account2   245.45 CHF\nIncome:Gains Assets:Account2 10000 USD",
                NaiveDate::from_ymd(2020, 1, 30),
                Annotated(Transaction::new(
                    NaiveDate::from_ymd(2020, 1, 30),
                    "some description".into(),
                    vec![Tag::new("tag1".into()), Tag::new("tag2".into())],
                    vec![
                        Posting {
                            credit: Account::new(AccountType::Assets, vec!["Account1".into()]),
                            debit: Account::new(AccountType::Assets, vec!["Account2".into()]),
                            amount: Decimal::new(24_545, 2),
                            commodity: Commodity::new("CHF".into()),
                            lot: None,
                            targets:None,
                         },
                         Posting {
                            credit: Account::new(AccountType::Income, vec!["Gains".into()]),
                            debit: Account::new(AccountType::Assets, vec!["Account2".into()]),
                            amount: Decimal::new(1_000_000, 2),
                            commodity: Commodity::new("USD".into()),
                            lot: None,
                            targets:None,
                         },
                    ],), (0, 115)),
            ),
        ];
        for (test, date, want) in tests {
            let mut s = Scanner::new(test);
            assert_eq!(parse_transaction(date, &mut s).unwrap(), want);
        }
    }

    #[test]
    fn test_parse_postings() {
        let tests = [(
            "Assets:Account1    Assets:Account2   4.00    CHF\nAssets:Account2    Assets:Account1   3.00 USD",
            Annotated(vec![
                Posting {
                    credit: Account::new(AccountType::Assets, vec!["Account1".into()]),
                    debit: Account::new(AccountType::Assets, vec!["Account2".into()]),
                    amount: Decimal::new(400, 2),
                    commodity: Commodity::new("CHF".into()),
                    lot: None,
                    targets:None,
                },
                Posting {
                    credit: Account::new(AccountType::Assets, vec!["Account2".into()]),
                    debit: Account::new(AccountType::Assets, vec!["Account1".into()]),
                    amount: Decimal::new(300, 2),
                    commodity: Commodity::new("USD".into()),
                    lot: None,
                    targets:None,
                }],
                (0, 94),
            ),
        )];
        for (test, want) in tests {
            let mut s = Scanner::new(test);
            assert_eq!(parse_postings(&mut s).unwrap(), want);
        }
    }

    #[test]
    fn test_parse_targets() {
        let mut s = Scanner::new("(A,B,  C   )");
        let got = parse_targets(&mut s).unwrap();
        assert_eq!(
            got,
            Annotated(
                vec![
                    Commodity::new("A".into()),
                    Commodity::new("B".into()),
                    Commodity::new("C".into())
                ],
                (0, 12)
            )
        );
    }

    #[test]
    fn test_parse_posting() {
        let tests = [(
            "Assets:Account1    Assets:Account2   4.00    CHF",
            Annotated(
                Posting {
                    credit: Account::new(AccountType::Assets, vec!["Account1".into()]),
                    debit: Account::new(AccountType::Assets, vec!["Account2".into()]),
                    amount: Decimal::new(400, 2),
                    commodity: Commodity::new("CHF".into()),
                    lot: None,
                    targets: None,
                },
                (0, 48),
            ),
        )];
        for (test, want) in tests {
            let mut s = Scanner::new(test);
            assert_eq!(parse_posting(&mut s).unwrap(), want);
        }
    }

    #[test]
    fn test_parse_price() {
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
            assert_eq!(parse_price(d, &mut s).unwrap(), want)
        }
    }

    #[test]
    fn test_parse_assertion() {
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
            assert_eq!(parse_assertion(d, &mut s).unwrap(), want)
        }
    }
}
