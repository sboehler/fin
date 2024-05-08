use std::rc::Rc;

use super::cst::{
    Account, Addon, Assertion, Booking, Commodity, Date, Decimal, Directive, QuotedString, Rng,
    SyntaxTree, Token,
};
use super::error::SyntaxError;
use super::file::File;
use crate::syntax::scanner::Scanner;

pub type Result<T> = std::result::Result<T, SyntaxError>;

pub struct Parser<'a> {
    scanner: Scanner<'a>,
}

impl<'a> Parser<'a> {
    pub fn new(s: &'a Rc<File>) -> Parser<'a> {
        Parser {
            scanner: Scanner::new(s),
        }
    }

    fn error(&self, pos: usize, msg: Option<String>, want: Token, got: Token) -> SyntaxError {
        SyntaxError::new(self.scanner.source.clone(), pos, msg, want, got)
    }

    pub fn parse_account(&self) -> Result<Account> {
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
            range: self.scanner.rng(start),
            segments,
        })
    }

    pub fn parse_commodity(&self) -> Result<Commodity> {
        self.scanner
            .read_identifier()
            .map(Commodity)
            .map_err(|e| e.update("parsing commodity"))
    }

    pub fn parse_date(&self) -> Result<Date> {
        let start = self.scanner.pos();
        self.scanner
            .read_n_with(4, Token::Digit, |c| c.is_ascii_digit())
            .map_err(|e| e.update("parsing year"))?;
        self.scanner.read_char('-')?;
        self.scanner
            .read_n_with(2, Token::Digit, |c| c.is_ascii_digit())
            .map_err(|e| e.update("parsing month"))?;
        self.scanner.read_char('-')?;
        self.scanner
            .read_n_with(2, Token::Digit, |c| c.is_ascii_digit())
            .map_err(|e| e.update("parsing day"))?;
        Ok(Date(self.scanner.rng(start)))
    }

    pub fn parse_interval(&self) -> Result<Rng> {
        let start = self.scanner.pos();
        match self.scanner.current() {
            Some('d') => self.scanner.read_string("daily"),
            Some('w') => self.scanner.read_string("weekly"),
            Some('m') => self.scanner.read_string("monthly"),
            Some('q') => self.scanner.read_string("quarterly"),
            Some('y') => self.scanner.read_string("yearly"),
            Some('o') => self.scanner.read_string("once"),
            o => Err(self.error(start, None, Token::Interval, Token::from_char(o))),
        }
    }

    pub fn parse_decimal(&self) -> Result<Decimal> {
        let start = self.scanner.pos();
        if let Some('-') = self.scanner.current() {
            self.scanner.read_char('-')?;
        }
        self.scanner
            .read_while_1(Token::Digit, |c| c.is_ascii_digit())?;
        if let Some('.') = self.scanner.current() {
            self.scanner.read_char('.')?;
            self.scanner
                .read_while_1(Token::Digit, |c| c.is_ascii_digit())?;
        }
        Ok(Decimal(self.scanner.rng(start)))
    }

    pub fn parse_quoted_string(&self) -> Result<QuotedString> {
        let start = self.scanner.pos();
        self.scanner.read_char('"')?;
        let content = self.scanner.read_while(|c| c != '"');
        self.scanner.read_char('"')?;
        Ok(QuotedString {
            range: self.scanner.rng(start),
            content,
        })
    }

    pub fn parse(&self) -> Result<SyntaxTree> {
        let start = self.scanner.pos();
        let mut directives = Vec::new();
        while let Some(c) = self.scanner.current() {
            match c {
                '*' | '/' | '#' => {
                    self.parse_comment()
                        .map_err(|e| e.update("parsing comment"))?;
                }
                c if c.is_alphanumeric() || c == '@' => {
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
                c if c.is_whitespace() => {
                    self.scanner
                        .read_rest_of_line()
                        .map_err(|e| e.update("parsing blank line"))?;
                }
                o => {
                    return Err(self.error(
                        start,
                        None,
                        Token::Either(vec![Token::Directive, Token::Comment, Token::BlankLine]),
                        Token::Char(o),
                    ))
                }
            }
        }
        Ok(SyntaxTree {
            range: self.scanner.rng(start),
            directives,
        })
    }

    pub fn parse_comment(&self) -> Result<Rng> {
        let start = self.scanner.pos();
        match self.scanner.current() {
            Some('#') | Some('*') => {
                self.scanner.read_until(|c| c == '\n');
                let range = self.scanner.rng(start);
                self.scanner.read_rest_of_line()?;
                Ok(range)
            }
            Some('/') => {
                self.scanner.read_string("//")?;
                self.scanner.read_until(|c| c == '\n');
                let range = self.scanner.rng(start);
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

    pub fn parse_directive(&self) -> Result<Directive> {
        match self.scanner.current() {
            Some('i') => self.parse_include(),
            Some(c) if c.is_ascii_digit() || c == '@' => self.parse_command().map_err(|e| {
                self.error(
                    self.scanner.pos(),
                    Some("parsing command".into()),
                    Token::Directive,
                    Token::Error(Box::new(e)),
                )
            }),
            o => Err(self.error(
                self.scanner.pos(),
                None,
                Token::Custom("directive".into()),
                Token::from_char(o),
            )),
        }
    }

    pub fn parse_include(&self) -> Result<Directive> {
        let start = self.scanner.pos();
        self.scanner.read_string("include")?;
        self.scanner.read_space1()?;
        let path = self
            .parse_quoted_string()
            .map_err(|e| e.update("parsing path"))?;
        Ok(Directive::Include {
            range: self.scanner.rng(start),
            path,
        })
    }

    pub fn parse_command(&self) -> Result<Directive> {
        let start = self.scanner.pos();
        let mut addon = None;
        if let Some('@') = self.scanner.current() {
            addon = Some(self.parse_addon()?);
            self.scanner.read_rest_of_line()?;
        }
        let date = self.parse_date().map_err(|e| e.update("parsing date"))?;
        self.scanner.read_space1()?;

        let command = match self.scanner.current() {
            Some('p') => self.parse_price(start, date).map_err(|e| {
                self.error(
                    start,
                    Some("parsing 'price' directive".into()),
                    Token::Custom("directive".into()),
                    Token::Error(Box::new(e)),
                )
            })?,
            Some('o') => self.parse_open(start, date).map_err(|e| {
                self.error(
                    start,
                    Some("parsing 'open' directive".into()),
                    Token::Custom("directive".into()),
                    Token::Error(Box::new(e)),
                )
            })?,
            Some('"') => self.parse_transaction(start, addon, date).map_err(|e| {
                self.error(
                    start,
                    Some("parsing 'transaction' directive".into()),
                    Token::Custom("directive".into()),
                    Token::Error(Box::new(e)),
                )
            })?,
            Some('b') => self.parse_assertion(start, date).map_err(|e| {
                self.error(
                    start,
                    Some("parsing 'balance' directive".into()),
                    Token::Custom("directive".into()),
                    Token::Error(Box::new(e)),
                )
            })?,
            Some('c') => self.parse_close(start, date).map_err(|e| {
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
                    Token::from_char(o),
                ))
            }
        };
        self.scanner.read_rest_of_line()?;
        Ok(command)
    }

    pub fn parse_addon(&self) -> Result<Addon> {
        let start = self.scanner.pos();
        self.scanner.read_char('@')?;
        let name = self.scanner.read_while_1(
            Token::Either(vec![Token::Custom("@performance".into())]),
            |c| c.is_alphabetic(),
        )?;
        match name.text() {
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

    pub fn parse_performance(&self, start: usize) -> Result<Addon> {
        self.scanner.read_space();
        self.scanner.read_char('(')?;
        self.scanner.read_space();
        let mut commodities = Vec::new();
        while self.scanner.current().map_or(false, char::is_alphanumeric) {
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
        Ok(Addon::Performance {
            range: self.scanner.rng(start),
            commodities,
        })
    }

    pub fn parse_accrual(&self, start: usize) -> Result<Addon> {
        self.scanner.read_space1()?;
        let interval = self
            .parse_interval()
            .map_err(|e| e.update("parsing interval"))?;
        self.scanner.read_space1()?;
        let start_date = self
            .parse_date()
            .map_err(|e| e.update("parsing start date"))?;
        self.scanner.read_space1()?;
        let end_date = self
            .parse_date()
            .map_err(|e| e.update("parsing end date"))?;
        self.scanner.read_space1()?;
        let account = self
            .parse_account()
            .map_err(|e| e.update("parsing accrual account"))?;
        Ok(Addon::Accrual {
            range: self.scanner.rng(start),
            interval,
            start: start_date,
            end: end_date,
            account,
        })
    }

    pub fn parse_price(&self, start: usize, date: Date) -> Result<Directive> {
        self.scanner.read_string("price")?;
        self.scanner.read_space1()?;
        let commodity = self
            .parse_commodity()
            .map_err(|e| e.update("parsing commodity"))?;
        self.scanner.read_space1()?;
        let price = self
            .parse_decimal()
            .map_err(|e| e.update("parsing price"))?;
        self.scanner.read_space1()?;
        let target = self
            .parse_commodity()
            .map_err(|e| e.update("parsing target commodity"))?;
        Ok(Directive::Price {
            range: self.scanner.rng(start),
            date,
            commodity,
            price,
            target,
        })
    }

    pub fn parse_open(&self, start: usize, date: Date) -> Result<Directive> {
        self.scanner.read_string("open")?;
        self.scanner.read_space1()?;
        let a = self
            .parse_account()
            .map_err(|e| e.update("parsing account"))?;
        Ok(Directive::Open {
            range: self.scanner.rng(start),
            date,
            account: a,
        })
    }

    pub fn parse_transaction(
        &self,
        start: usize,
        addon: Option<Addon>,
        date: Date,
    ) -> Result<Directive> {
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
            if !self.scanner.current().map_or(false, char::is_alphanumeric) {
                break;
            }
        }
        Ok(Directive::Transaction {
            range: self.scanner.rng(start),
            addon,
            date,
            description,
            bookings,
        })
    }

    pub fn parse_booking(&self) -> Result<Booking> {
        let start = self.scanner.pos();
        let credit = self
            .parse_account()
            .map_err(|e| e.update("parsing credit account"))?;
        self.scanner.read_space1()?;
        let debit = self
            .parse_account()
            .map_err(|e| e.update("parsing debit account"))?;
        self.scanner.read_space1()?;
        let quantity = self
            .parse_decimal()
            .map_err(|e| e.update("parsing quantity"))?;
        self.scanner.read_space1()?;
        let commodity = self
            .parse_commodity()
            .map_err(|e| e.update("parsing commodity"))?;
        Ok(Booking {
            range: self.scanner.rng(start),
            credit,
            debit,
            quantity,
            commodity,
        })
    }

    pub fn parse_assertion(&self, start: usize, date: Date) -> Result<Directive> {
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
                if !self.scanner.current().map_or(false, char::is_alphanumeric) {
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
        Ok(Directive::Assertion {
            range: self.scanner.rng(start),
            date,
            assertions,
        })
    }

    pub fn parse_sub_assertion(&self) -> Result<Assertion> {
        let start = self.scanner.pos();
        let account = self
            .parse_account()
            .map_err(|e| e.update("parsing account"))?;
        self.scanner.read_space1()?;
        let amount = self
            .parse_decimal()
            .map_err(|e| e.update("parsing amount"))?;
        self.scanner.read_space1()?;
        let commodity = self
            .parse_commodity()
            .map_err(|e| e.update("parsing commodity"))?;
        Ok(Assertion {
            range: self.scanner.rng(start),
            account,
            balance: amount,
            commodity,
        })
    }

    pub fn parse_close(&self, start: usize, date: Date) -> Result<Directive> {
        self.scanner.read_string("close")?;
        self.scanner.read_space1()?;
        let account = self
            .parse_account()
            .map_err(|e| e.update("parsing account"))?;
        Ok(Directive::Close {
            range: self.scanner.rng(start),
            date,
            account,
        })
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use crate::syntax::cst::Rng;

    use super::*;

    #[test]
    fn test_parse_commodity() {
        let f1 = File::mem("USD");
        assert_eq!(
            Ok(Commodity(Rng::new(f1.clone(), 0, 3))),
            Parser::new(&f1).parse_commodity(),
        );
        let f2 = File::mem("1FOO");
        assert_eq!(
            Ok(Commodity(Rng::new(f2.clone(), 0, 4))),
            Parser::new(&File::mem("1FOO")).parse_commodity()
        );
        let f3 = File::mem(" USD");
        assert_eq!(
            Err(SyntaxError::new(
                f3.clone(),
                0,
                Some("parsing commodity".into()),
                Token::AlphaNum,
                Token::WhiteSpace
            )),
            Parser::new(&f3).parse_commodity()
        );
        let f4 = File::mem("/USD");
        assert_eq!(
            Err(SyntaxError::new(
                f4.clone(),
                0,
                Some("parsing commodity".into()),
                Token::AlphaNum,
                Token::Char('/')
            )),
            Parser::new(&f4).parse_commodity()
        );
    }

    #[test]
    fn test_parse_account() {
        let f1 = File::mem("Sometype");
        assert_eq!(
            Ok(Account {
                range: Rng::new(f1.clone(), 0, 8),
                segments: vec![Rng::new(f1.clone(), 0, 8)],
            }),
            Parser::new(&f1).parse_account(),
        );
        let f2 = File::mem("Liabilities:Debt  ");
        assert_eq!(
            Ok(Account {
                range: Rng::new(f2.clone(), 0, 16),
                segments: vec![Rng::new(f2.clone(), 0, 11), Rng::new(f2.clone(), 12, 16)],
            }),
            Parser::new(&f2).parse_account(),
        );
        let f3 = File::mem(" USD");
        assert_eq!(
            Err(SyntaxError::new(
                f3.clone(),
                0,
                Some("parsing account type".into()),
                Token::AlphaNum,
                Token::WhiteSpace
            )),
            Parser::new(&f3).parse_account(),
        );
        let f4 = File::mem("/USD");
        assert_eq!(
            Err(SyntaxError::new(
                f4.clone(),
                0,
                Some("parsing account type".into()),
                Token::AlphaNum,
                Token::Char('/')
            )),
            Parser::new(&f4).parse_account(),
        );
    }

    #[test]
    fn test_parse_date() {
        let f1 = File::mem("2024-05-07");
        assert_eq!(
            Ok(Date(Rng::new(f1.clone(), 0, 10))),
            Parser::new(&f1).parse_date(),
        );
        let f2 = File::mem("024-02-02");
        assert_eq!(
            Err(SyntaxError::new(
                f2.clone(),
                3,
                Some("parsing year".into()),
                Token::Digit,
                Token::Char('-')
            )),
            Parser::new(&f2).parse_date(),
        );
        let f3 = File::mem("2024-02-0");
        assert_eq!(
            Err(SyntaxError::new(
                f3.clone(),
                9,
                Some("parsing day".into()),
                Token::Digit,
                Token::EOF
            )),
            Parser::new(&f3).parse_date(),
        );
        let f4 = File::mem("2024-0--0");
        assert_eq!(
            Err(SyntaxError::new(
                f4.clone(),
                6,
                Some("parsing month".into()),
                Token::Digit,
                Token::Char('-')
            )),
            Parser::new(&f4).parse_date()
        )
    }

    #[test]
    fn test_parse_interval() {
        for d in ["daily", "weekly", "monthly", "quarterly", "yearly", "once"] {
            assert_eq!(
                Ok(d),
                Parser::new(&File::mem(d))
                    .parse_interval()
                    .as_ref()
                    .map(Rng::text),
            );
        }
    }

    #[test]
    fn test_parse_decimal() {
        for d in ["0", "10.01", "-10.01"] {
            let f = File::mem(d);
            assert_eq!(
                Ok(Decimal(Rng::new(f.clone(), 0, d.len()))),
                Parser::new(&f).parse_decimal(),
            );
        }

        let f = File::mem("foo");
        assert_eq!(
            Err(SyntaxError::new(
                f.clone(),
                0,
                None,
                Token::Digit,
                Token::Char('f')
            )),
            Parser::new(&f).parse_decimal(),
        );
    }

    mod addon {
        use crate::syntax::parser::Parser;
        use crate::syntax::{
            cst::{Account, Addon, Commodity, Date, Rng},
            file::File,
        };
        use pretty_assertions::assert_eq;

        #[test]
        fn performance() {
            let f1 = File::mem("@performance( USD  , VT)");
            assert_eq!(
                Ok(Addon::Performance {
                    range: Rng::new(f1.clone(), 0, 24),

                    commodities: vec![
                        Commodity(Rng::new(f1.clone(), 14, 17)),
                        Commodity(Rng::new(f1.clone(), 21, 23)),
                    ]
                }),
                Parser::new(&f1).parse_addon()
            );
            let f2 = File::mem("@performance(  )");
            assert_eq!(
                Ok(Addon::Performance {
                    range: Rng::new(f2.clone(), 0, 16),
                    commodities: vec![]
                }),
                Parser::new(&f2).parse_addon(),
            )
        }

        #[test]
        fn accrual() {
            let f = File::mem("@accrue monthly 2024-01-01 2024-12-31 Assets:Payables");
            assert_eq!(
                Ok(Addon::Accrual {
                    range: Rng::new(f.clone(), 0, 53),
                    interval: Rng::new(f.clone(), 8, 15),
                    start: Date(Rng::new(f.clone(), 16, 26)),
                    end: Date(Rng::new(f.clone(), 27, 37)),
                    account: Account {
                        range: Rng::new(f.clone(), 38, 53),
                        segments: vec![Rng::new(f.clone(), 38, 44), Rng::new(f.clone(), 45, 53)]
                    }
                }),
                Parser::new(&f).parse_addon()
            )
        }
    }

    #[test]
    fn test_parse_open() {
        let f = File::mem("open   Assets:Foo");
        assert_eq!(
            Ok(Directive::Open {
                range: Rng::new(f.clone(), 0, 17),
                date: Date(Rng::new(f.clone(), 0, 0)),
                account: Account {
                    range: Rng::new(f.clone(), 7, 17),
                    segments: vec![Rng::new(f.clone(), 7, 13), Rng::new(f.clone(), 14, 17)]
                }
            }),
            Parser::new(&f).parse_open(0, Date(Rng::new(f.clone(), 0, 0)))
        )
    }

    #[test]
    fn test_parse_booking() {
        let f = File::mem("Assets:Foo Assets:Bar 4.23 BAZ");

        assert_eq!(
            Ok(Booking {
                range: Rng::new(f.clone(), 0, 30),
                credit: Account {
                    range: Rng::new(f.clone(), 0, 10),
                    segments: vec![Rng::new(f.clone(), 0, 6), Rng::new(f.clone(), 7, 10)]
                },
                debit: Account {
                    range: Rng::new(f.clone(), 11, 21),
                    segments: vec![Rng::new(f.clone(), 11, 17), Rng::new(f.clone(), 18, 21)]
                },
                quantity: Decimal(Rng::new(f.clone(), 22, 26)),
                commodity: Commodity(Rng::new(f.clone(), 27, 30)),
            }),
            Parser::new(&f).parse_booking()
        )
    }

    #[test]
    fn test_parse_transaction() {
        let f =
            File::mem("\"Message\"  \nAssets:Foo Assets:Bar 4.23 USD\nAssets:Foo Assets:Baz 8 USD");
        assert_eq!(
            Ok(Directive::Transaction {
                range: Rng::new(f.clone(), 0, 70),
                addon: None,
                date: Date(Rng::new(f.clone(), 0, 0)),
                description: QuotedString {
                    range: Rng::new(f.clone(), 0, 9),
                    content: Rng::new(f.clone(), 1, 8),
                },
                bookings: vec![
                    Booking {
                        range: Rng::new(f.clone(), 12, 42),
                        credit: Account {
                            range: Rng::new(f.clone(), 12, 22),
                            segments: vec![
                                Rng::new(f.clone(), 12, 18),
                                Rng::new(f.clone(), 19, 22)
                            ]
                        },
                        debit: Account {
                            range: Rng::new(f.clone(), 23, 33),
                            segments: vec![
                                Rng::new(f.clone(), 23, 29),
                                Rng::new(f.clone(), 30, 33)
                            ]
                        },
                        quantity: Decimal(Rng::new(f.clone(), 34, 38)),
                        commodity: Commodity(Rng::new(f.clone(), 39, 42)),
                    },
                    Booking {
                        range: Rng::new(f.clone(), 43, 70),
                        credit: Account {
                            range: Rng::new(f.clone(), 43, 53),
                            segments: vec![
                                Rng::new(f.clone(), 43, 49),
                                Rng::new(f.clone(), 50, 53)
                            ]
                        },
                        debit: Account {
                            range: Rng::new(f.clone(), 54, 64),
                            segments: vec![
                                Rng::new(f.clone(), 54, 60),
                                Rng::new(f.clone(), 61, 64)
                            ]
                        },
                        quantity: Decimal(Rng::new(f.clone(), 65, 66)),
                        commodity: Commodity(Rng::new(f.clone(), 67, 70)),
                    }
                ]
            }),
            Parser::new(&f).parse_transaction(0, None, Date(Rng::new(f.clone(), 0, 0)))
        );
    }
    #[test]
    fn test_parse_transaction2() {
        let f = File::mem("\"");
        assert_eq!(
            Err(SyntaxError::new(
                f.clone(),
                1,
                None,
                Token::Char('"'),
                Token::EOF
            ),),
            Parser::new(&f).parse_transaction(0, None, Date(Rng::new(f.clone(), 0, 0)))
        );
    }
    #[test]
    fn test_parse_transaction3() {
        let f = File::mem("\"\"   Assets Assets 12 USD");
        assert_eq!(
            Err(SyntaxError::new(
                f.clone(),
                5,
                None,
                Token::Either(vec![Token::Char('\n'), Token::EOF]),
                Token::Char('A'),
            ),),
            Parser::new(&f).parse_transaction(0, None, Date(Rng::new(f.clone(), 0, 25)))
        )
    }
    #[test]
    fn test_parse_close() {
        let f = File::mem("close  Assets:Foo");
        assert_eq!(
            Ok(Directive::Close {
                range: Rng::new(f.clone(), 0, 17),
                date: Date(Rng::new(f.clone(), 0, 0)),
                account: Account {
                    range: Rng::new(f.clone(), 7, 17),
                    segments: vec![Rng::new(f.clone(), 7, 13), Rng::new(f.clone(), 14, 17)]
                }
            }),
            Parser::new(&f).parse_close(0, Date(Rng::new(f.clone(), 0, 0)))
        )
    }

    mod directive {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn parse_include() {
            let f = File::mem(r#"include "/foo/bar/baz/finance.knut""#);
            assert_eq!(
                Ok(Directive::Include {
                    range: Rng::new(f.clone(), 0, 35),
                    path: QuotedString {
                        range: Rng::new(f.clone(), 8, 35),
                        content: Rng::new(f.clone(), 9, 34),
                    }
                }),
                Parser::new(&f).parse_directive()
            )
        }

        #[test]
        fn parse_open() {
            let f = File::mem("2024-03-01 open Assets:Foo");
            assert_eq!(
                Ok(Directive::Open {
                    range: Rng::new(f.clone(), 0, 26),
                    date: Date(Rng::new(f.clone(), 0, 10)),
                    account: Account {
                        range: Rng::new(f.clone(), 16, 26),
                        segments: vec![Rng::new(f.clone(), 16, 22), Rng::new(f.clone(), 23, 26)]
                    },
                }),
                Parser::new(&f).parse_directive()
            )
        }

        #[test]
        fn parse_transaction() {
            let f = File::mem("2024-12-31 \"Message\"  \nAssets:Foo Assets:Bar 4.23 USD");
            assert_eq!(
                Ok(Directive::Transaction {
                    range: Rng::new(f.clone(), 0, 53),
                    addon: None,
                    date: Date(Rng::new(f.clone(), 0, 10)),
                    description: QuotedString {
                        range: Rng::new(f.clone(), 11, 20),
                        content: Rng::new(f.clone(), 12, 19),
                    },
                    bookings: vec![Booking {
                        range: Rng::new(f.clone(), 23, 53),
                        credit: Account {
                            range: Rng::new(f.clone(), 23, 33),
                            segments: vec![
                                Rng::new(f.clone(), 23, 29),
                                Rng::new(f.clone(), 30, 33)
                            ]
                        },
                        debit: Account {
                            range: Rng::new(f.clone(), 34, 44),
                            segments: vec![
                                Rng::new(f.clone(), 34, 40),
                                Rng::new(f.clone(), 41, 44)
                            ]
                        },
                        quantity: Decimal(Rng::new(f.clone(), 45, 49)),
                        commodity: Commodity(Rng::new(f.clone(), 50, 53)),
                    },]
                }),
                Parser::new(&f).parse_directive()
            );
        }

        #[test]
        fn parse_close() {
            let f = File::mem("2024-03-01 close Assets:Foo");
            assert_eq!(
                Ok(Directive::Close {
                    range: Rng::new(f.clone(), 0, 27),
                    date: Date(Rng::new(f.clone(), 0, 10)),
                    account: Account {
                        range: Rng::new(f.clone(), 17, 27),
                        segments: vec![Rng::new(f.clone(), 17, 23), Rng::new(f.clone(), 24, 27)]
                    }
                }),
                Parser::new(&f).parse_directive()
            )
        }

        #[test]
        fn parse_price() {
            let f = File::mem("2024-03-01 price FOO 1.543 BAR");
            assert_eq!(
                Ok(Directive::Price {
                    range: Rng::new(f.clone(), 0, 30),
                    date: Date(Rng::new(f.clone(), 0, 10)),
                    commodity: Commodity(Rng::new(f.clone(), 17, 20)),
                    price: Decimal(Rng::new(f.clone(), 21, 26)),
                    target: Commodity(Rng::new(f.clone(), 27, 30)),
                }),
                Parser::new(&f).parse_directive()
            )
        }

        #[test]
        fn parse_assertion() {
            let f = File::mem("2024-03-01 balance Assets:Foo 500.1 BAR");
            assert_eq!(
                Ok(Directive::Assertion {
                    range: Rng::new(f.clone(), 0, 39),
                    date: Date(Rng::new(f.clone(), 0, 10)),
                    assertions: vec![Assertion {
                        range: Rng::new(f.clone(), 19, 39),
                        account: Account {
                            range: Rng::new(f.clone(), 19, 29),
                            segments: vec![
                                Rng::new(f.clone(), 19, 25),
                                Rng::new(f.clone(), 26, 29)
                            ],
                        },
                        balance: Decimal(Rng::new(f.clone(), 30, 35)),
                        commodity: Commodity(Rng::new(f.clone(), 36, 39)),
                    }]
                }),
                Parser::new(&f).parse_directive()
            )
        }
    }
}
