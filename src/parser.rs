use crate::context::Context;
use crate::model::{
    Account, Accrual, Assertion, Close, Command, Commodity, Interval, Lot, Open, Period, Posting,
    PostingBuilder, Price, Tag, Transaction, Value,
};
use crate::scanner::{Annotated, Character, Result, Scanner};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use std::fmt;
use std::fmt::Display;
use std::path::PathBuf;
use std::sync::Arc;

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
    context: Arc<Context>,
    scanner: Scanner<'a>,
}

impl<'a> Iterator for Parser<'a> {
    type Item = Result<Directive>;

    fn next(&mut self) -> Option<Self::Item> {
        self.parse_directive().transpose()
    }
}

impl<'a> Parser<'a> {
    pub fn new(s: &str) -> Parser {
        Parser {
            context: Arc::new(Context::new()),
            scanner: Scanner::new(s),
        }
    }

    pub fn new_from_file(context: Arc<Context>, s: &str, filename: Option<PathBuf>) -> Parser {
        Parser {
            context,
            scanner: Scanner::new_from_file(s, filename),
        }
    }

    fn parse_directive(&mut self) -> Result<Option<Directive>> {
        while self.scanner.current().is_some() {
            self.scanner.skip_while(|c| c.is_ascii_whitespace());
            if let Some(c) = self.scanner.current() {
                match c {
                    '0'..='9' | '@' => {
                        let c = self.parse_command()?;
                        return Ok(Some(Directive::Command(c.0)));
                    }
                    'i' => {
                        return Ok(Some(self.parse_include()?.0));
                    }
                    '*' | '#' => {
                        self.scanner.consume_rest_of_line()?;
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
        Ok(None)
    }

    pub fn parse_command(&mut self) -> Result<Annotated<Command>> {
        let pos = self.scanner.pos();
        let cmd;
        if let Some('@') = self.scanner.current() {
            let a = self.parse_accrual()?.0;
            let d = self.parse_date()?.0;
            self.scanner.consume_space1()?;
            cmd = Command::Trx(self.parse_transaction(d, Some(a))?.0);
        } else {
            let d = self.parse_date()?.0;
            self.scanner.consume_space1()?;
            cmd = match self.scanner.current() {
                Some('p') => Command::Price(self.parse_price(d)?.0),
                Some('"') => Command::Trx(self.parse_transaction(d, None)?.0),
                Some('o') => Command::Open(self.parse_open(d)?.0),
                Some('b') => Command::Assertion(self.parse_assertion(d)?.0),
                Some('c') => Command::Close(self.parse_close(d)?.0),
                Some('v') => Command::Value(self.parse_value(d)?.0),
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

    fn parse_accrual(&mut self) -> Result<Annotated<Accrual>> {
        let pos = self.scanner.pos();
        self.scanner.consume_string("@accrue")?;
        self.scanner.consume_space1()?;
        let interval = self.parse_interval()?.0;
        self.scanner.consume_space1()?;
        let start = self.parse_date()?.0;
        self.scanner.consume_space1()?;
        let end = self.parse_date()?.0;
        self.scanner.consume_space1()?;
        let account = self.parse_account()?.0;
        self.scanner.consume_space1()?;
        self.scanner.consume_eol()?;
        self.scanner.annotate(
            pos,
            Accrual {
                period: Period { start, end },
                interval,
                account,
            },
        )
    }

    fn parse_open(&mut self, d: NaiveDate) -> Result<Annotated<Open>> {
        let pos = self.scanner.pos();
        self.scanner.consume_string("open")?;
        self.scanner.consume_space1()?;
        let a = self.parse_account()?.0;
        self.scanner.consume_space1()?;
        self.scanner.consume_eol()?;
        self.scanner.annotate(
            pos,
            Open {
                date: d,
                account: a,
            },
        )
    }

    fn parse_close(&mut self, d: NaiveDate) -> Result<Annotated<Close>> {
        let pos = self.scanner.pos();
        self.scanner.consume_string("close")?;
        self.scanner.consume_space1()?;
        let a = self.parse_account()?.0;
        self.scanner.consume_space1()?;
        self.scanner.consume_eol()?;
        self.scanner.annotate(
            pos,
            Close {
                date: d,
                account: a,
            },
        )
    }

    fn parse_value(&mut self, d: NaiveDate) -> Result<Annotated<Value>> {
        let pos = self.scanner.pos();
        self.scanner.consume_string("value")?;
        self.scanner.consume_space1()?;
        let acc = self.parse_account()?.0;
        self.scanner.consume_space1()?;
        let amt = self.parse_decimal()?.0;
        self.scanner.consume_space1()?;
        let com = self.parse_commodity()?.0;
        self.scanner.consume_space1()?;
        self.scanner.consume_eol()?;
        self.scanner.annotate(pos, Value::new(d, acc, amt, com))
    }

    fn parse_transaction(
        &mut self,
        d: NaiveDate,
        acc: Option<Accrual>,
    ) -> Result<Annotated<Transaction>> {
        let pos = self.scanner.pos();
        let desc = self.scanner.read_quoted_string()?.0;
        self.scanner.consume_space1()?;
        let tags = self.parse_tags()?.0;
        self.scanner.consume_eol()?;
        let postings = self.parse_postings()?.0;
        let t = Transaction::new(d, desc.into(), tags, postings, acc);
        self.scanner.annotate(pos, t)
    }

    fn parse_postings(&mut self) -> Result<Annotated<Vec<Posting>>> {
        let pos = self.scanner.pos();
        let mut postings = Vec::new();
        postings.extend(self.parse_posting()?.0);
        while self
            .scanner
            .current()
            .map(|c| c.is_ascii_alphanumeric())
            .unwrap_or(false)
        {
            postings.extend(self.parse_posting()?.0)
        }
        self.scanner.annotate(pos, postings)
    }

    fn parse_posting(&mut self) -> Result<Annotated<Vec<Posting>>> {
        let pos = self.scanner.pos();
        let mut lot = None;
        let mut targets = None;
        let credit = self.parse_account()?.0;

        self.scanner.consume_space1()?;
        let debit = self.parse_account()?.0;
        self.scanner.consume_space1()?;
        let amount = self.parse_decimal()?.0;
        self.scanner.consume_space1()?;
        let commodity = self.parse_commodity()?.0;
        self.scanner.consume_space1()?;
        if let Some('(') = self.scanner.current() {
            targets = Some(self.parse_targets()?.0);
            self.scanner.consume_space1()?;
        }
        if let Some('{') = self.scanner.current() {
            lot = Some(self.parse_lot()?);
            self.scanner.consume_space1()?;
        }
        let posting = self.scanner.annotate(
            pos,
            PostingBuilder {
                credit,
                debit,
                commodity,
                amount,
                value: Decimal::ZERO,
                lot,
                targets,
            }
            .build(),
        );
        self.scanner.consume_eol()?;
        posting
    }

    fn parse_assertion(&mut self, d: NaiveDate) -> Result<Annotated<Assertion>> {
        let pos = self.scanner.pos();
        self.scanner.consume_string("balance")?;
        self.scanner.consume_space1()?;
        let account = self.parse_account()?.0;
        self.scanner.consume_space1()?;
        let price = self.parse_decimal()?.0;
        self.scanner.consume_space1()?;
        let commodity = self.parse_commodity()?.0;
        self.scanner.consume_space1()?;
        self.scanner.consume_eol()?;
        self.scanner
            .annotate(pos, Assertion::new(d, account, price, commodity))
    }

    fn parse_account(&mut self) -> Result<Annotated<Arc<Account>>> {
        let pos = self.scanner.pos();
        let s = self.scanner.read_until(char::is_whitespace).0;
        match self.context.account(s) {
            Ok(a) => self.scanner.annotate(pos, a),
            Err(e) => Err(self.scanner.error(
                pos,
                Some("error parsing account".into()),
                Character::Custom("account".into()),
                Character::Custom(e),
            )),
        }
    }

    fn parse_tags(&mut self) -> Result<Annotated<Vec<Tag>>> {
        let pos = self.scanner.pos();
        let mut v = Vec::new();
        while let Some('#') = self.scanner.current() {
            v.push(self.parse_tag()?.0);
            self.scanner.consume_space1()?.0
        }
        self.scanner.annotate(pos, v)
    }

    fn parse_tag(&mut self) -> Result<Annotated<Tag>> {
        let pos = self.scanner.pos();
        self.scanner.consume_char('#')?;
        let tag = self.scanner.read_identifier()?.0;
        self.scanner.annotate(pos, Tag::new(tag.into()))
    }

    fn parse_interval(&mut self) -> Result<Annotated<Interval>> {
        let pos = self.scanner.pos();
        let str = self.scanner.read_identifier()?.0;
        match str.to_lowercase().as_str() {
            "once" => self.scanner.annotate(pos, Interval::Once),
            "daily" => self.scanner.annotate(pos, Interval::Daily),
            "weekly" => self.scanner.annotate(pos, Interval::Weekly),
            "monthly" => self.scanner.annotate(pos, Interval::Monthly),
            "quarterly" => self.scanner.annotate(pos, Interval::Quarterly),
            "yearly" => self.scanner.annotate(pos, Interval::Yearly),
            _ => Err(self.scanner.error(
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

    fn parse_date(&mut self) -> Result<Annotated<NaiveDate>> {
        let pos = self.scanner.pos();
        let b = self.scanner.read_n(10)?;
        match NaiveDate::parse_from_str(b.0, "%Y-%m-%d") {
            Ok(d) => self.scanner.annotate(pos, d),
            Err(_) => Err(self.scanner.error(
                pos,
                Some("error parsing date".into()),
                Character::Custom("date (YYYY-MM-DD)".into()),
                Character::Custom(b.0.into()),
            )),
        }
    }

    fn parse_decimal(&mut self) -> Result<Annotated<Decimal>> {
        let pos = self.scanner.pos();
        let t = self.scanner.read_until(|c| c.is_whitespace()).0;
        match t.parse::<Decimal>() {
            Ok(d) => self.scanner.annotate(pos, d),
            Err(_) => Err(self.scanner.error(
                pos,
                Some("error parsing decimal".into()),
                Character::Custom("a decimal value".into()),
                Character::Custom(t.to_string()),
            )),
        }
    }

    fn parse_commodity(&mut self) -> Result<Annotated<Arc<Commodity>>> {
        let pos = self.scanner.pos();
        let s = self.scanner.read_identifier()?.0;
        match self.context.commodity(s) {
            Ok(a) => self.scanner.annotate(pos, a),
            Err(e) => Err(self.scanner.error(
                pos,
                Some("error parsing commodity".into()),
                Character::Custom("commodity".into()),
                Character::Custom(e),
            )),
        }
    }

    fn parse_lot(&mut self) -> Result<Lot> {
        let pos = self.scanner.pos();
        self.scanner.consume_char('{')?;
        self.scanner.consume_space1()?;
        let price = self.parse_decimal()?.0;
        self.scanner.consume_space1()?;
        let commodity = self.parse_commodity()?.0;
        let mut label = None;
        let mut date = None;
        self.scanner.consume_space();
        while let Some(',') = self.scanner.current() {
            self.scanner.consume_char(',')?;
            self.scanner.consume_space();
            match self.scanner.current() {
                Some('"') => {
                    label = Some(self.scanner.read_quoted_string()?);
                    self.scanner.consume_space();
                }
                Some(d) if d.is_ascii_digit() => {
                    date = Some(self.parse_date()?.0);
                    self.scanner.consume_space();
                }
                c => {
                    return Err(self.scanner.error(
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
        self.scanner.consume_char('}')?;
        Ok(Lot::new(
            price,
            commodity,
            date,
            label.map(|s| s.0.to_string()),
        ))
    }

    fn parse_targets(&mut self) -> Result<Annotated<Vec<Arc<Commodity>>>> {
        let pos = self.scanner.pos();
        let mut targets = Vec::new();
        self.scanner.consume_char('(')?;
        self.scanner.consume_space();
        if Some(')') != self.scanner.current() {
            targets.push(self.parse_commodity()?.0);
            self.scanner.consume_space();
        }
        while let Some(',') = self.scanner.current() {
            self.scanner.consume_char(',')?;
            self.scanner.consume_space();
            targets.push(self.parse_commodity()?.0);
            self.scanner.consume_space();
        }
        self.scanner.consume_char(')')?;
        self.scanner.annotate(pos, targets)
    }

    fn parse_price(&mut self, d: NaiveDate) -> Result<Annotated<Price>> {
        let pos = self.scanner.pos();
        self.scanner.consume_string("price")?;
        self.scanner.consume_space1()?;
        let source = self.parse_commodity()?.0;
        self.scanner.consume_space1()?;
        let price = self.parse_decimal()?.0;
        self.scanner.consume_space1()?;
        let target = self.parse_commodity()?.0;
        self.scanner.consume_space1()?;
        self.scanner.consume_eol()?;
        self.scanner
            .annotate(pos, Price::new(d, price, target, source))
    }

    fn parse_include(&mut self) -> Result<Annotated<Directive>> {
        let pos = self.scanner.pos();
        self.scanner.consume_string("include")?;
        self.scanner.consume_space1()?;
        let directive = self
            .scanner
            .read_quoted_string()
            .map(|a| a.0)
            .map(std::path::PathBuf::from)
            .map(Directive::Include)?;
        self.scanner.consume_space1()?;
        self.scanner.consume_eol()?;
        self.scanner.annotate(pos, directive)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::AccountType;

    #[test]
    fn test_parse_date() {
        assert_eq!(
            Parser::new("0202-02-02").parse_date().unwrap(),
            Annotated(NaiveDate::from_ymd_opt(202, 2, 2).unwrap(), (0, 10))
        );
        assert_eq!(
            Parser::new("2020-09-15").parse_date().unwrap(),
            Annotated(
                NaiveDate::from_ymd_opt(2020, 9, 15).unwrap(),
                (0, 10)
            )
        );
    }

    #[test]
    fn test_parse_account() {
        assert_eq!(
            Parser::new("Assets").parse_account().unwrap(),
            Annotated(Account::new(AccountType::Assets, &[]), (0, 6))
        );
        assert_eq!(
            Parser::new("Liabilities:CreditCards:Visa")
                .parse_account()
                .unwrap(),
            Annotated(
                Account::new(AccountType::Liabilities, &["CreditCards", "Visa"]),
                (0, 28)
            )
        );
    }

    #[test]
    fn test_parse_open() {
        assert_eq!(
            Parser::new("open Assets:Account")
                .parse_open(NaiveDate::from_ymd_opt(2020, 2, 2).unwrap())
                .unwrap(),
            Annotated(
                Open {
                    date: NaiveDate::from_ymd_opt(2020, 2, 2).unwrap(),
                    account: Arc::new(Account {
                        account_type: AccountType::Assets,
                        segments: vec!["Account".into()],
                    }),
                },
                (0, 19)
            )
        )
    }

    #[test]
    fn test_parse_close() {
        assert_eq!(
            Parser::new("close Assets:Account")
                .parse_close(NaiveDate::from_ymd_opt(2020, 2, 2).unwrap())
                .unwrap(),
            Annotated(
                Close {
                    date: NaiveDate::from_ymd_opt(2020, 2, 2).unwrap(),
                    account: Arc::new(Account {
                        account_type: AccountType::Assets,
                        segments: vec!["Account".into()],
                    }),
                },
                (0, 20)
            )
        )
    }

    #[test]
    fn test_parse_value() {
        assert_eq!(
            Parser::new("value  Assets:Account  101.40 KNUT")
                .parse_value(NaiveDate::from_ymd_opt(2020, 2, 2).unwrap())
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
            Parser::new("#tag1 #1tag   no more tags")
                .parse_tags()
                .unwrap(),
            Annotated(
                vec![Tag::new("tag1".into()), Tag::new("1tag".into())],
                (0, 14)
            )
        );
    }

    #[test]
    fn test_parse_decimal() {
        assert_eq!(
            Parser::new("3.14").parse_decimal().unwrap(),
            Annotated(Decimal::new(314, 2), (0, 4))
        );
        assert_eq!(
            Parser::new("-3.141").parse_decimal().unwrap(),
            Annotated(Decimal::new(-3141, 3), (0, 6))
        );
        assert_eq!(
            Parser::new("3.14159265359").parse_decimal().unwrap(),
            Annotated(Decimal::new(314159265359, 11), (0, 13))
        );
    }

    #[test]
    fn test_parse_transaction() {
        assert_eq!(
            Parser::new("\"some description\"\nAssets:Account1 Expenses:Trading 245.45 CHF (ABC)\nIncome:Gains1 Assets:Foo -245.45 CHF")
                .parse_transaction(NaiveDate::from_ymd_opt(2020, 1, 30).unwrap(), None).unwrap(),
            Annotated(Transaction::new(
                    NaiveDate::from_ymd_opt(2020, 1, 30).unwrap(),
                    "some description".into(),
                    vec![],
                    vec![
                        Posting {
                            account: Account::new(AccountType::Assets, &["Account1"]),
                            other: Account::new(AccountType::Expenses, &["Trading"]),
                            amount: Decimal::new(-24545, 2),
                            value: Decimal::ZERO,
                            commodity: Commodity::new("CHF".into()),
                            lot: None,
                            targets: Some(vec![Commodity::new("ABC".into())]),
                        },
                        Posting {
                            account: Account::new(AccountType::Expenses, &["Trading"]),
                            other: Account::new(AccountType::Assets, &["Account1"]),
                            amount: Decimal::new(24545, 2),
                            value: Decimal::ZERO,
                            commodity: Commodity::new("CHF".into()),
                            lot: None,
                            targets: Some(vec![Commodity::new("ABC".into())]),
                        },
                        Posting {
                            account: Account::new(AccountType::Assets, &["Foo"]),
                            other: Account::new(AccountType::Income, &["Gains1"]),
                            amount: Decimal::new(-24545, 2),
                            value: Decimal::ZERO,
                            commodity:             Commodity::new("CHF".into()),
                            lot: None,
                            targets:None,
                        },
                        Posting {
                            account: Account::new(AccountType::Income, &["Gains1"]),
                            other: Account::new(AccountType::Assets, &["Foo"]),
                            amount: Decimal::new(24545, 2),
                            value: Decimal::ZERO,
                            commodity:             Commodity::new("CHF".into()),
                            lot: None,
                            targets:None,
                        },
                    ],
                    None,
                ), (0, 105)),
            );
        assert_eq!(
            Parser::new("\"some description\" #tag1 #tag2 \nAssets:Account1 Assets:Account2   245.45 CHF\nIncome:Gains Assets:Account2 10000 USD")
                .parse_transaction(NaiveDate::from_ymd_opt(2020, 1, 30).unwrap(), None).unwrap(),
            Annotated(Transaction::new(
                    NaiveDate::from_ymd_opt(2020, 1, 30).unwrap(),
                    "some description".into(),
                    vec![Tag::new("tag1".into()), Tag::new("tag2".into())],
                    vec![
                        Posting {
                            account: Account::new(AccountType::Assets, &["Account1"]),
                            other: Account::new(AccountType::Assets, &["Account2"]),
                            amount: Decimal::new(-24_545, 2),
                            value: Decimal::ZERO,
                            commodity: Commodity::new("CHF".into()),
                            lot: None,
                            targets:None,
                        },
                        Posting {
                            other: Account::new(AccountType::Assets, &["Account1"]),
                            account: Account::new(AccountType::Assets, &["Account2"]),
                            amount: Decimal::new(24_545, 2),
                            value: Decimal::ZERO,
                            commodity: Commodity::new("CHF".into()),
                            lot: None,
                            targets:None,
                         },
                         Posting {
                             account: Account::new(AccountType::Income, &["Gains"]),
                             other: Account::new(AccountType::Assets, &["Account2"]),
                             amount: Decimal::new(-1_000_000, 2),
                             value: Decimal::ZERO,
                             commodity: Commodity::new("USD".into()),
                             lot: None,
                             targets:None,
                        },
                        Posting {
                               other: Account::new(AccountType::Income, &["Gains"]),
                               account: Account::new(AccountType::Assets, &["Account2"]),
                               amount: Decimal::new(1_000_000, 2),
                               value: Decimal::ZERO,
                               commodity: Commodity::new("USD".into()),
                               lot: None,
                               targets:None,
                        },
                    ],None), (0, 115)))
    }

    #[test]
    fn test_parse_accrual() {
        assert_eq!(
            Parser::new("@accrue daily 2022-04-03 2022-05-05 Liabilities:Accruals")
                .parse_accrual()
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
            Parser::new("@accrue monthly 2022-04-03     2022-05-05     Liabilities:Bank    ")
                .parse_accrual()
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
            Parser::new("daily").parse_interval().unwrap(),
            Annotated(Interval::Daily, (0, 5))
        );
        assert_eq!(
            Parser::new("weekly").parse_interval().unwrap(),
            Annotated(Interval::Weekly, (0, 6))
        );
        assert_eq!(
            Parser::new("monthly").parse_interval().unwrap(),
            Annotated(Interval::Monthly, (0, 7))
        );
        assert_eq!(
            Parser::new("quarterly").parse_interval().unwrap(),
            Annotated(Interval::Quarterly, (0, 9))
        );
        assert_eq!(
            Parser::new("yearly").parse_interval().unwrap(),
            Annotated(Interval::Yearly, (0, 6))
        );
        assert_eq!(
            Parser::new("once").parse_interval().unwrap(),
            Annotated(Interval::Once, (0, 4))
        );
    }

    #[test]
    fn test_parse_postings() {
        assert_eq!(
            Parser::new("Assets:Account1    Assets:Account2   4.00    CHF\nAssets:Account2    Assets:Account1   3.00 USD").parse_postings().unwrap(),
            Annotated(
                vec![
                    Posting {
                        account: Account::new(AccountType::Assets, &["Account1"]),
                        other: Account::new(AccountType::Assets, &["Account2"]),
                        amount: Decimal::new(-400, 2),
                        value: Decimal::ZERO,
                        commodity: Commodity::new("CHF".into()),
                        lot: None,
                        targets: None,
                    },
                    Posting {
                        other: Account::new(AccountType::Assets, &["Account1"]),
                        account: Account::new(AccountType::Assets, &["Account2"]),
                        amount: Decimal::new(400, 2),
                        value: Decimal::ZERO,
                        commodity: Commodity::new("CHF".into()),
                        lot: None,
                        targets: None,
                    },
                    Posting {
                        account: Account::new(AccountType::Assets, &["Account2"]),
                        other: Account::new(AccountType::Assets, &["Account1"]),
                        amount: Decimal::new(-300, 2),
                        value: Decimal::ZERO,
                        commodity: Commodity::new("USD".into()),
                        lot: None,
                        targets: None,
                    },
                    Posting {
                        other: Account::new(AccountType::Assets, &["Account2"]),
                        account: Account::new(AccountType::Assets, &["Account1"]),
                        amount: Decimal::new(300, 2),
                        value: Decimal::ZERO,
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
            Parser::new("(A,B,  C   )").parse_targets().unwrap(),
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
            Parser::new("Assets:Account1    Assets:Account2   4.00    CHF")
                .parse_posting()
                .unwrap(),
            Annotated(
                vec![
                    Posting {
                        account: Account::new(AccountType::Assets, &["Account1"]),
                        other: Account::new(AccountType::Assets, &["Account2"]),
                        amount: Decimal::new(-400, 2),
                        value: Decimal::ZERO,
                        commodity: Commodity::new("CHF".into()),
                        lot: None,
                        targets: None,
                    },
                    Posting {
                        account: Account::new(AccountType::Assets, &["Account2"]),
                        other: Account::new(AccountType::Assets, &["Account1"]),
                        amount: Decimal::new(400, 2),
                        value: Decimal::ZERO,
                        commodity: Commodity::new("CHF".into()),
                        lot: None,
                        targets: None,
                    },
                ],
                (0, 48),
            )
        );
    }

    #[test]
    fn test_parse_price() {
        let date = NaiveDate::from_ymd_opt(2020, 2, 2).unwrap();
        assert_eq!(
            Parser::new("price USD 0.901 CHF")
                .parse_price(date)
                .unwrap(),
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
            Parser::new("price 1MDB 1000000000 USD")
                .parse_price(date,)
                .unwrap(),
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
            Parser::new("balance Assets:MyAccount 0.901 USD")
                .parse_assertion(d,)
                .unwrap(),
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
            Parser::new("balance Liabilities:123foo 100 1CT")
                .parse_assertion(d,)
                .unwrap(),
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
