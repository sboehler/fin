use crate::syntax::scanner::{Result, Scanner, Token};
use std::path::PathBuf;

use super::scanner::{ParserError, Range1};
use super::syntax::{
    Account, Addon, Assertion, Booking, Command, Commodity, Date, Decimal,
    Directive, QuotedString, SourceFile,
};

pub struct Parser<'a> {
    scanner: Scanner<'a>,
}

impl<'a> Parser<'a> {
    pub fn new(s: &'a str) -> Parser<'a> {
        Parser {
            scanner: Scanner::new(s),
        }
    }

    pub fn new_from_file(
        s: &'a str,
        filename: Option<&'a PathBuf>,
    ) -> Parser<'a> {
        Parser {
            scanner: Scanner::new_from_file(s, filename),
        }
    }

    fn error(
        &self,
        pos: usize,
        msg: Option<String>,
        want: Token,
        got: Token,
    ) -> ParserError {
        ParserError::new(
            &self.scanner.source,
            self.scanner.filename,
            pos,
            msg,
            want,
            got,
        )
    }

    pub fn parse_account(&self) -> Result<Account<'a>> {
        let start = self.scanner.pos();
        let account_type = self
            .scanner
            .read_identifier()
            .map_err(|e| e.update("parsing account type"))?;
        let mut segments = vec![account_type];
        while self.scanner.current() == Some(':') {
            self.scanner.read_char(':')?;
            segments.push(
                self.scanner
                    .read_identifier()
                    .map_err(|e| e.update("parsing account segment"))?,
            );
        }
        Ok(Account {
            range: self.scanner.range_from(start),
            segments,
        })
    }

    pub fn parse_commodity(&self) -> Result<Commodity<'a>> {
        self.scanner
            .read_identifier()
            .map_err(|e| e.update("parsing commodity"))
            .map(Commodity)
    }

    pub fn parse_date(&self) -> Result<Date<'a>> {
        let start = self.scanner.pos();
        self.scanner
            .read_n_with(4, Token::Digit, |c| c.is_ascii_digit())
            .map_err(|e| e.update("parsing year".into()))?;
        self.scanner.read_char('-')?;
        self.scanner
            .read_n_with(2, Token::Digit, |c| c.is_ascii_digit())
            .map_err(|e| e.update("parsing month".into()))?;
        self.scanner.read_char('-')?;
        self.scanner
            .read_n_with(2, Token::Digit, |c| c.is_ascii_digit())
            .map_err(|e| e.update("parsing day".into()))?;
        Ok(Date(self.scanner.range_from(start)))
    }

    pub fn parse_interval(&self) -> Result<Range1<'a>> {
        let start = self.scanner.pos();
        match self.scanner.current() {
            Some('d') => self.scanner.read_string("daily"),
            Some('w') => self.scanner.read_string("weekly"),
            Some('m') => self.scanner.read_string("monthly"),
            Some('q') => self.scanner.read_string("quarterly"),
            Some('y') => self.scanner.read_string("yearly"),
            Some('o') => self.scanner.read_string("once"),
            o => Err(self.error(
                start,
                None,
                Token::Interval,
                o.map_or(Token::EOF, Token::Char),
            )),
        }
    }

    pub fn parse_decimal(&self) -> Result<Decimal<'a>> {
        let start = self.scanner.pos();
        if let Some('-') = self.scanner.current() {
            self.scanner.read_char('-')?;
        }
        self.scanner.read_while_1(Token::Digit, |c| c.is_ascii_digit())?;
        if let Some('.') = self.scanner.current() {
            self.scanner.read_char('.')?;
            self.scanner.read_while_1(Token::Digit, |c| c.is_ascii_digit())?;
        }
        Ok(Decimal(self.scanner.range_from(start)))
    }

    pub fn parse_quoted_string(&self) -> Result<QuotedString<'a>> {
        let start = self.scanner.pos();
        self.scanner.read_char('"')?;
        let content = self.scanner.read_while(|c| c != '"');
        self.scanner.read_char('"')?;
        Ok(QuotedString {
            range: self.scanner.range_from(start),
            content,
        })
    }

    pub fn parse_file(&self) -> Result<SourceFile<'a>> {
        let start = self.scanner.pos();
        let mut directives = Vec::new();
        while self.scanner.current().is_some() {
            match self.scanner.current() {
                Some('*') | Some('/') | Some('#') => {
                    self.parse_comment()
                        .map_err(|e| e.update("parsing comment"))?;
                }
                Some(c) if c.is_alphanumeric() || c == '@' => {
                    let d = self.parse_directive().map_err(|e| {
                        self.error(
                            self.scanner.pos(),
                            Some("parsing directive".into()),
                            Token::Directive,
                            Token::Error(Box::new(e)),
                        )
                    })?;
                    directives.push(d)
                }
                Some(c) if c.is_whitespace() => {
                    self.scanner
                        .read_rest_of_line()
                        .map_err(|e| e.update("parsing blank line"))?;
                }
                o => {
                    return Err(self.error(
                        start,
                        None,
                        Token::Either(vec![
                            Token::Directive,
                            Token::Comment,
                            Token::BlankLine,
                        ]),
                        o.map_or(Token::EOF, Token::Char),
                    ))
                }
            }
        }
        Ok(SourceFile {
            range: self.scanner.range_from(start),
            directives,
        })
    }

    pub fn parse_comment(&self) -> Result<Range1<'a>> {
        let start = self.scanner.pos();
        match self.scanner.current() {
            Some('#') | Some('*') => {
                self.scanner.read_until(|c| c == '\n');
                let range = self.scanner.range_from(start);
                self.scanner.read_rest_of_line()?;
                Ok(range)
            }
            Some('/') => {
                self.scanner.read_string("//")?;
                self.scanner.read_until(|c| c == '\n');
                let range = self.scanner.range_from(start);
                self.scanner.read_rest_of_line()?;
                Ok(range)
            }
            o => Err(self.error(
                start,
                None,
                Token::Comment,
                o.map_or(Token::EOF, Token::Char),
            )),
        }
    }

    pub fn parse_directive(&self) -> Result<Directive<'a>> {
        match self.scanner.current() {
            Some('i') => self.parse_include(),
            Some(c) if c.is_ascii_digit() || c == '@' => {
                self.parse_command().map_err(|e| {
                    self.error(
                        self.scanner.pos(),
                        Some("parsing command".into()),
                        Token::Directive,
                        Token::Error(Box::new(e)),
                    )
                })
            }
            o => Err(self.error(
                self.scanner.pos(),
                None,
                Token::Custom("directive".into()),
                o.map(Token::Char).unwrap_or(Token::EOF),
            )),
        }
    }

    pub fn parse_include(&self) -> Result<Directive<'a>> {
        let start = self.scanner.pos();
        self.scanner.read_string("include")?;
        self.scanner.read_space1()?;
        let path =
            self.parse_quoted_string().map_err(|e| e.update("parsing path"))?;
        Ok(Directive::Include {
            range: self.scanner.range_from(start),
            path,
        })
    }

    pub fn parse_command(&self) -> Result<Directive<'a>> {
        let start = self.scanner.pos();
        let mut addons = Vec::new();
        while let Some('@') = self.scanner.current() {
            addons.push(self.parse_addon()?);
            self.scanner.read_rest_of_line()?;
        }
        let date = self.parse_date().map_err(|e| e.update("parsing date"))?;
        self.scanner.read_space1()?;
        let command = match self.scanner.current() {
            Some('p') => self.parse_price().map_err(|e| {
                self.error(
                    start,
                    Some("parsing 'price' directive".into()),
                    Token::Custom("directive".into()),
                    Token::Error(Box::new(e)),
                )
            })?,
            Some('o') => self.parse_open().map_err(|e| {
                self.error(
                    start,
                    Some("parsing 'open' directive".into()),
                    Token::Custom("directive".into()),
                    Token::Error(Box::new(e)),
                )
            })?,
            Some('"') => self.parse_transaction().map_err(|e| {
                self.error(
                    start,
                    Some("parsing 'transaction' directive".into()),
                    Token::Custom("directive".into()),
                    Token::Error(Box::new(e)),
                )
            })?,
            Some('b') => self.parse_assertion().map_err(|e| {
                self.error(
                    start,
                    Some("parsing 'balance' directive".into()),
                    Token::Custom("directive".into()),
                    Token::Error(Box::new(e)),
                )
            })?,
            Some('c') => self.parse_close().map_err(|e| {
                self.error(
                    start,
                    Some("parsing 'close' directive".into()),
                    Token::Custom("directive".into()),
                    Token::Error(Box::new(e)),
                )
            })?,
            o => {
                return Err(self.error(
                    self.scanner.pos(),
                    None,
                    Token::Either(vec![
                        Token::Custom("price".into()),
                        Token::Custom("open".into()),
                        Token::Custom("balance".into()),
                        Token::Custom("opening quote (\")".into()),
                        Token::Custom("close".into()),
                    ]),
                    o.map(Token::Char).unwrap_or(Token::EOF),
                ))
            }
        };
        let range = self.scanner.range_from(start);
        self.scanner.read_rest_of_line()?;
        Ok(Directive::Dated {
            range,
            addons,
            date,
            command,
        })
    }

    pub fn parse_addonified_transaction(&self) -> Result<Command<'a>> {
        let mut addons = Vec::new();
        loop {
            addons.push(self.parse_addon()?);
            self.scanner.read_rest_of_line()?;
            if self.scanner.current().map(|c| c != '@').unwrap_or(false) {
                break;
            }
        }
        self.parse_transaction()
    }

    pub fn parse_addon(&self) -> Result<Addon<'a>> {
        let start = self.scanner.pos();
        self.scanner.read_char('@')?;
        let name = self.scanner.read_while_1(
            Token::Either(vec![Token::Custom("@performance".into())]),
            |c| c.is_alphabetic(),
        )?;
        match name.str {
            "performance" => self
                .parse_performance(start)
                .map_err(|e| e.update("parsing performance")),
            "accrue" => self
                .parse_accrual(start)
                .map_err(|e| e.update("parsing accrual")),
            o => Err(self.error(
                self.scanner.pos(),
                Some("parsing addon".into()),
                Token::Either(vec![Token::Custom("@performance".into())]),
                Token::Custom(o.into()),
            )),
        }
    }

    pub fn parse_performance(&self, start: usize) -> Result<Addon<'a>> {
        self.scanner.read_space();
        self.scanner.read_char('(')?;
        self.scanner.read_space();
        let mut commodities = Vec::new();
        while self
            .scanner
            .current()
            .map(|c| c.is_alphanumeric())
            .unwrap_or(false)
        {
            commodities.push(
                self.parse_commodity()
                    .map_err(|e| e.update("parsing commodity"))?,
            );
            self.scanner.read_space();
            if let Some(',') = self.scanner.current() {
                self.scanner.read_char(',')?;
                self.scanner.read_space();
            }
        }
        self.scanner.read_char(')')?;
        let range = self.scanner.range_from(start);
        Ok(Addon::Performance {
            range,
            commodities,
        })
    }

    pub fn parse_accrual(&self, start: usize) -> Result<Addon<'a>> {
        self.scanner.read_space1()?;
        let interval =
            self.parse_interval().map_err(|e| e.update("parsing interval"))?;
        self.scanner.read_space1()?;
        let start_date =
            self.parse_date().map_err(|e| e.update("parsing start date"))?;
        self.scanner.read_space1()?;
        let end_date =
            self.parse_date().map_err(|e| e.update("parsing end date"))?;
        self.scanner.read_space1()?;
        let account = self
            .parse_account()
            .map_err(|e| e.update("parsing accrual account"))?;
        Ok(Addon::Accrual {
            range: self.scanner.range_from(start),
            interval: interval,
            start: start_date,
            end: end_date,
            account: account,
        })
    }

    pub fn parse_price(&self) -> Result<Command<'a>> {
        let start = self.scanner.pos();
        self.scanner.read_string("price")?;
        self.scanner.read_space1()?;
        let commodity = self
            .parse_commodity()
            .map_err(|e| e.update("parsing commodity"))?;
        self.scanner.read_space1()?;
        let price =
            self.parse_decimal().map_err(|e| e.update("parsing price"))?;
        self.scanner.read_space1()?;
        let target = self
            .parse_commodity()
            .map_err(|e| e.update("parsing target commodity"))?;
        Ok(Command::Price {
            range: self.scanner.range_from(start),
            commodity,
            price,
            target,
        })
    }

    pub fn parse_open(&self) -> Result<Command<'a>> {
        let start = self.scanner.pos();
        self.scanner.read_string("open")?;
        self.scanner.read_space1()?;
        let a =
            self.parse_account().map_err(|e| e.update("parsing account"))?;
        Ok(Command::Open {
            range: self.scanner.range_from(start),
            account: a,
        })
    }

    pub fn parse_transaction(&self) -> Result<Command<'a>> {
        let start = self.scanner.pos();
        let description = self.parse_quoted_string()?;
        self.scanner.read_rest_of_line()?;
        let mut bookings = Vec::new();
        loop {
            bookings.push(self.parse_booking().map_err(|e| {
                self.error(
                    self.scanner.pos(),
                    Some("parsing booking".into()),
                    Token::Custom("booking".into()),
                    Token::Error(Box::new(e)),
                )
            })?);
            self.scanner.read_rest_of_line()?;
            if !self.scanner.current().map_or(false, |c| c.is_alphanumeric()) {
                break;
            }
        }
        Ok(Command::Transaction {
            range: self.scanner.range_from(start),
            description,
            bookings,
        })
    }

    pub fn parse_booking(&self) -> Result<Booking<'a>> {
        let start = self.scanner.pos();
        let credit = self
            .parse_account()
            .map_err(|e| e.update("parsing credit account"))?;
        self.scanner.read_space1()?;
        let debit = self
            .parse_account()
            .map_err(|e| e.update("parsing debit account"))?;
        self.scanner.read_space1()?;
        let quantity =
            self.parse_decimal().map_err(|e| e.update("parsing quantity"))?;
        self.scanner.read_space1()?;
        let commodity = self
            .parse_commodity()
            .map_err(|e| e.update("parsing commodity"))?;
        let range = self.scanner.range_from(start);
        Ok(Booking {
            range: range,
            credit,
            debit,
            quantity,
            commodity,
        })
    }

    pub fn parse_assertion(&self) -> Result<Command<'a>> {
        let start = self.scanner.pos();
        self.scanner.read_string("balance")?;
        self.scanner.read_space1()?;
        let mut assertions = Vec::new();
        if let Some('\n') = self.scanner.current() {
            self.scanner.read_rest_of_line()?;
            loop {
                assertions.push(self.parse_sub_assertion().map_err(|e| {
                    self.error(
                        self.scanner.pos(),
                        Some("parsing assertion".into()),
                        Token::Custom("assertion".into()),
                        Token::Error(Box::new(e)),
                    )
                })?);
                self.scanner.read_rest_of_line()?;
                if !self
                    .scanner
                    .current()
                    .map_or(false, |c| c.is_alphanumeric())
                {
                    break;
                }
            }
        } else {
            assertions.push(self.parse_sub_assertion().map_err(|e| {
                self.error(
                    self.scanner.pos(),
                    Some("parsing assertion".into()),
                    Token::Custom("assertion".into()),
                    Token::Error(Box::new(e)),
                )
            })?);
        }
        Ok(Command::Assertion {
            range: self.scanner.range_from(start),
            assertions: assertions,
        })
    }

    pub fn parse_sub_assertion(&self) -> Result<Assertion<'a>> {
        let start = self.scanner.pos();
        let account =
            self.parse_account().map_err(|e| e.update("parsing account"))?;
        self.scanner.read_space1()?;
        let amount =
            self.parse_decimal().map_err(|e| e.update("parsing amount"))?;
        self.scanner.read_space1()?;
        let commodity = self
            .parse_commodity()
            .map_err(|e| e.update("parsing commodity"))?;
        Ok(Assertion {
            range: self.scanner.range_from(start),
            account,
            amount,
            commodity,
        })
    }

    pub fn parse_close(&self) -> Result<Command<'a>> {
        let start = self.scanner.pos();
        self.scanner.read_string("close")?;
        self.scanner.read_space1()?;
        let a = self
            .parse_account()
            .map_err(|e| e.update("parsing account").into())?;
        Ok(Command::Close {
            range: self.scanner.range_from(start),
            account: a,
        })
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use crate::syntax::scanner::Range1;

    use super::*;

    #[test]
    fn test_parse_commodity() {
        assert_eq!(
            Ok(Commodity(Range1::new(0, "USD"))),
            Parser::new("USD").parse_commodity(),
        );
        assert_eq!(
            Ok(Commodity(Range1::new(0, "1FOO"))),
            Parser::new("1FOO  ").parse_commodity(),
        );
        assert_eq!(
            Err(ParserError::new(
                " USD",
                None,
                0,
                Some("parsing commodity".into()),
                Token::AlphaNum,
                Token::WhiteSpace
            )),
            Parser::new(" USD").parse_commodity()
        );
        assert_eq!(
            Err(ParserError::new(
                "/USD",
                None,
                0,
                Some("parsing commodity".into()),
                Token::AlphaNum,
                Token::Char('/')
            )),
            Parser::new("/USD").parse_commodity()
        );
    }

    #[test]
    fn test_parse_account() {
        assert_eq!(
            Ok(Account {
                range: Range1::new(0, "Sometype"),
                segments: vec![Range1::new(0, "Sometype")],
            }),
            Parser::new("Sometype").parse_account(),
        );
        assert_eq!(
            Ok(Account {
                range: Range1::new(0, "Liabilities:Debt"),
                segments: vec![
                    Range1::new(0, "Liabilities"),
                    Range1::new(12, "Debt")
                ],
            }),
            Parser::new("Liabilities:Debt  ").parse_account(),
        );
        assert_eq!(
            Err(ParserError::new(
                " USD",
                None,
                0,
                Some("parsing account type".into()),
                Token::AlphaNum,
                Token::WhiteSpace
            )),
            Parser::new(" USD").parse_account(),
        );
        assert_eq!(
            Err(ParserError::new(
                "/USD",
                None,
                0,
                Some("parsing account type".into()),
                Token::AlphaNum,
                Token::Char('/')
            )),
            Parser::new("/USD").parse_account(),
        );
    }

    #[test]
    fn test_parse_date() {
        assert_eq!(
            Ok(Date(Range1::new(0, "0202-02-02"))),
            Parser::new("0202-02-02").parse_date(),
        );
        assert_eq!(
            Ok(Date(Range1::new(0, "2024-02-02"))),
            Parser::new("2024-02-02").parse_date(),
        );
        assert_eq!(
            Err(ParserError::new(
                "024-02-02",
                None,
                3,
                Some("parsing year".into()),
                Token::Digit,
                Token::Char('-')
            )),
            Parser::new("024-02-02").parse_date(),
        );
        assert_eq!(
            Err(ParserError::new(
                "2024-02-0",
                None,
                9,
                Some("parsing day".into()),
                Token::Digit,
                Token::EOF
            )),
            Parser::new("2024-02-0").parse_date(),
        );
        assert_eq!(
            Err(ParserError::new(
                "2024-0--0",
                None,
                6,
                Some("parsing month".into()),
                Token::Digit,
                Token::Char('-')
            )),
            Parser::new("2024-0--0").parse_date()
        )
    }

    #[test]
    fn test_parse_interval() {
        assert_eq!(
            Ok(Range1::new(0, "daily")),
            Parser::new("daily").parse_interval(),
        );
        assert_eq!(
            Ok(Range1::new(0, "weekly")),
            Parser::new("weekly").parse_interval(),
        );
        assert_eq!(
            Ok(Range1::new(0, "monthly")),
            Parser::new("monthly").parse_interval(),
        );
        assert_eq!(
            Ok(Range1::new(0, "quarterly")),
            Parser::new("quarterly").parse_interval(),
        );
        assert_eq!(
            Ok(Range1::new(0, "yearly")),
            Parser::new("yearly").parse_interval(),
        );
        assert_eq!(
            Ok(Range1::new(0, "once")),
            Parser::new("once").parse_interval(),
        );
    }

    #[test]
    fn test_parse_decimal() {
        assert_eq!(
            Ok(Decimal(Range1::new(0, "0"))),
            Parser::new("0").parse_decimal(),
        );
        assert_eq!(
            Ok(Decimal(Range1::new(0, "10.01"))),
            Parser::new("10.01").parse_decimal(),
        );
        assert_eq!(
            Ok(Decimal(Range1::new(0, "-10.01"))),
            Parser::new("-10.01").parse_decimal(),
        );
        assert_eq!(
            Err(ParserError::new(
                "foo",
                None,
                0,
                None,
                Token::Digit,
                Token::Char('f')
            )),
            Parser::new("foo").parse_decimal(),
        );
    }

    mod addon {
        use crate::syntax::{
            parser::Parser,
            scanner::Range1,
            syntax::{Account, Addon, Commodity, Date},
        };
        use pretty_assertions::assert_eq;

        #[test]
        fn performance() {
            assert_eq!(
                Ok(Addon::Performance {
                    range: Range1::new(0, "@performance( USD  , VT)"),

                    commodities: vec![
                        Commodity(Range1::new(14, "USD")),
                        Commodity(Range1::new(21, "VT")),
                    ]
                }),
                Parser::new("@performance( USD  , VT)").parse_addon()
            );
            assert_eq!(
                Ok(Addon::Performance {
                    range: Range1::new(0, "@performance(  )"),
                    commodities: vec![]
                }),
                Parser::new("@performance(  )").parse_addon(),
            )
        }

        #[test]
        fn accrual() {
            assert_eq!(
                Ok(Addon::Accrual {
                    range: Range1::new(
                        0,
                        "@accrue monthly 2024-01-01 2024-12-31 Assets:Payables"
                    ),
                    interval: Range1::new(8, "monthly"),
                    start: Date(Range1::new(16, "2024-01-01")),
                    end: Date(Range1::new(27, "2024-12-31")),
                    account: Account {
                        range: Range1::new(38, "Assets:Payables"),
                        segments: vec![
                            Range1::new(38, "Assets"),
                            Range1::new(45, "Payables")
                        ]
                    }
                }),
                Parser::new(
                    "@accrue monthly 2024-01-01 2024-12-31 Assets:Payables"
                )
                .parse_addon()
            )
        }
    }

    #[test]
    fn test_parse_open() {
        assert_eq!(
            Ok(Command::Open {
                range: Range1::new(0, "open   Assets:Foo"),
                account: Account {
                    range: Range1::new(7, "Assets:Foo"),
                    segments: vec![
                        Range1::new(7, "Assets"),
                        Range1::new(14, "Foo")
                    ]
                }
            }),
            Parser::new("open   Assets:Foo").parse_open()
        )
    }

    #[test]
    fn test_parse_booking() {
        assert_eq!(
            Ok(Booking {
                range: Range1::new(0, "Assets:Foo Assets:Bar 4.23 BAZ"),
                credit: Account {
                    range: Range1::new(0, "Assets:Foo"),
                    segments: vec![
                        Range1::new(0, "Assets"),
                        Range1::new(7, "Foo")
                    ]
                },
                debit: Account {
                    range: Range1::new(11, "Assets:Bar"),
                    segments: vec![
                        Range1::new(11, "Assets"),
                        Range1::new(18, "Bar")
                    ]
                },
                quantity: Decimal(Range1::new(22, "4.23")),
                commodity: Commodity(Range1::new(27, "BAZ")),
            }),
            Parser::new("Assets:Foo Assets:Bar 4.23 BAZ").parse_booking()
        )
    }

    #[test]
    fn test_parse_transaction() {
        let s = "\"Message\"  \nAssets:Foo Assets:Bar 4.23 USD\nAssets:Foo Assets:Baz 8 USD";
        assert_eq!(
            Ok(Command::Transaction {
                range: Range1::new(0, s),
                description: QuotedString {
                    range: Range1::new(0, r#""Message""#),
                    content: Range1::new(1, "Message"),
                },
                bookings: vec![
                    Booking {
                        range: Range1::new(
                            12,
                            "Assets:Foo Assets:Bar 4.23 USD"
                        ),
                        credit: Account {
                            range: Range1::new(12, "Assets:Foo"),
                            segments: vec![
                                Range1::new(12, "Assets"),
                                Range1::new(19, "Foo")
                            ]
                        },
                        debit: Account {
                            range: Range1::new(23, "Assets:Bar"),
                            segments: vec![
                                Range1::new(23, "Assets"),
                                Range1::new(30, "Bar")
                            ]
                        },
                        quantity: Decimal(Range1::new(34, "4.23")),
                        commodity: Commodity(Range1::new(39, "USD")),
                    },
                    Booking {
                        range: Range1::new(43, "Assets:Foo Assets:Baz 8 USD"),
                        credit: Account {
                            range: Range1::new(43, "Assets:Foo"),
                            segments: vec![
                                Range1::new(43, "Assets"),
                                Range1::new(50, "Foo")
                            ]
                        },
                        debit: Account {
                            range: Range1::new(54, "Assets:Baz"),
                            segments: vec![
                                Range1::new(54, "Assets"),
                                Range1::new(61, "Baz")
                            ]
                        },
                        quantity: Decimal(Range1::new(65, "8")),
                        commodity: Commodity(Range1::new(67, "USD")),
                    }
                ]
            }),
            Parser::new(s).parse_transaction()
        );
    }
    #[test]
    fn test_parse_transaction2() {
        assert_eq!(
            Err(ParserError::new(
                "\"",
                None,
                1,
                None,
                Token::Char('"'),
                Token::EOF
            ),),
            Parser::new("\"").parse_transaction()
        );
    }
    #[test]
    fn test_parse_transaction3() {
        assert_eq!(
            Err(ParserError::new(
                "\"\"   Assets Assets 12 USD",
                None,
                5,
                None,
                Token::Either(vec![Token::Char('\n'), Token::EOF]),
                Token::Char('A'),
            ),),
            Parser::new("\"\"   Assets Assets 12 USD").parse_transaction()
        )
    }
    #[test]
    fn test_parse_close() {
        assert_eq!(
            Ok(Command::Close {
                range: Range1::new(0, "close  Assets:Foo"),
                account: Account {
                    range: Range1::new(7, "Assets:Foo"),
                    segments: vec![
                        Range1::new(7, "Assets"),
                        Range1::new(14, "Foo")
                    ]
                }
            }),
            Parser::new("close  Assets:Foo").parse_close()
        )
    }

    mod directive {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn parse_include() {
            assert_eq!(
                Ok(Directive::Include {
                    range: Range1::new(
                        0,
                        r#"include "/foo/bar/baz/finance.knut""#
                    ),
                    path: QuotedString {
                        range: Range1::new(8, r#""/foo/bar/baz/finance.knut""#),
                        content: Range1::new(9, "/foo/bar/baz/finance.knut"),
                    }
                }),
                Parser::new(r#"include "/foo/bar/baz/finance.knut""#)
                    .parse_directive()
            )
        }

        #[test]
        fn parse_open() {
            assert_eq!(
                Ok(Directive::Dated {
                    range: Range1::new(0, "2024-03-01 open Assets:Foo"),
                    addons: Vec::new(),
                    date: Date(Range1::new(0, "2024-03-01")),
                    command: Command::Open {
                        range: Range1::new(11, "open Assets:Foo"),
                        account: Account {
                            range: Range1::new(16, "Assets:Foo"),
                            segments: vec![
                                Range1::new(16, "Assets"),
                                Range1::new(23, "Foo")
                            ]
                        }
                    },
                }),
                Parser::new("2024-03-01 open Assets:Foo").parse_directive()
            )
        }

        #[test]
        fn parse_transaction() {
            assert_eq!(
                Ok(Directive::Dated {
                    range: Range1::new(0, "2024-12-31 \"Message\"  \nAssets:Foo Assets:Bar 4.23 USD"),
                    addons: Vec::new(),
                    date: Date (Range1::new(0, "2024-12-31")),
                    command: Command::Transaction {
                        range: Range1::new(
                            11,
                            "\"Message\"  \nAssets:Foo Assets:Bar 4.23 USD"
                        ),
                        description: QuotedString {
                            range: Range1::new(11, r#""Message""#),
                            content: Range1::new(12, "Message"),
                        },
                        bookings: vec![Booking {
                            range: Range1::new(23, "Assets:Foo Assets:Bar 4.23 USD"),
                            credit: Account {
                                range: Range1::new(23, "Assets:Foo"),
                                segments: vec![
                                    Range1::new(23, "Assets"),
                                    Range1::new(30, "Foo")
                                ]
                            },
                            debit: Account {
                                range: Range1::new(34, "Assets:Bar"),
                                segments: vec![
                                    Range1::new(34, "Assets"),
                                    Range1::new(41, "Bar")
                                ]
                            },
                            quantity: Decimal( Range1::new(45, "4.23")),
                            commodity: Commodity (Range1::new(50, "USD")),
                        },]
                    },
                }),
                Parser::new(
                    "2024-12-31 \"Message\"  \nAssets:Foo Assets:Bar 4.23 USD"
                )
                .parse_directive()
            );
        }

        #[test]
        fn parse_close() {
            assert_eq!(
                Ok(Directive::Dated {
                    range: Range1::new(0, "2024-03-01 close Assets:Foo"),
                    addons: Vec::new(),
                    date: Date(Range1::new(0, "2024-03-01")),
                    command: Command::Close {
                        range: Range1::new(11, "close Assets:Foo"),
                        account: Account {
                            range: Range1::new(17, "Assets:Foo"),
                            segments: vec![
                                Range1::new(17, "Assets"),
                                Range1::new(24, "Foo")
                            ]
                        }
                    },
                }),
                Parser::new("2024-03-01 close Assets:Foo").parse_directive()
            )
        }

        #[test]
        fn parse_price() {
            assert_eq!(
                Ok(Directive::Dated {
                    range: Range1::new(0, "2024-03-01 price FOO 1.543 BAR"),
                    addons: Vec::new(),
                    date: Date(Range1::new(0, "2024-03-01")),
                    command: Command::Price {
                        range: Range1::new(11, "price FOO 1.543 BAR"),
                        commodity: Commodity(Range1::new(17, "FOO")),
                        price: Decimal(Range1::new(21, "1.543")),
                        target: Commodity(Range1::new(27, "BAR")),
                    },
                }),
                Parser::new("2024-03-01 price FOO 1.543 BAR").parse_directive()
            )
        }

        #[test]
        fn parse_assertion() {
            assert_eq!(
                Ok(Directive::Dated {
                    range: Range1::new(
                        0,
                        "2024-03-01 balance Assets:Foo 500.1 BAR"
                    ),
                    addons: Vec::new(),
                    date: Date(Range1::new(0, "2024-03-01")),
                    command: Command::Assertion {
                        range: Range1::new(11, "balance Assets:Foo 500.1 BAR"),
                        assertions: vec![Assertion {
                            range: Range1::new(19, "Assets:Foo 500.1 BAR"),
                            account: Account {
                                range: Range1::new(19, "Assets:Foo"),
                                segments: vec![
                                    Range1::new(19, "Assets"),
                                    Range1::new(26, "Foo")
                                ],
                            },
                            amount: Decimal(Range1::new(30, "500.1")),
                            commodity: Commodity(Range1::new(36, "BAR")),
                        }]
                    },
                }),
                Parser::new("2024-03-01 balance Assets:Foo 500.1 BAR")
                    .parse_directive()
            )
        }
    }
}
