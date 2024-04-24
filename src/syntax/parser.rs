use crate::syntax::scanner::{Result, Scanner, Token};
use std::path::PathBuf;

use super::scanner::ParserError;
use super::syntax::{
    Account, Close, Command, Commodity, Date, Decimal, Directive, Open,
};

pub struct Parser<'a> {
    scanner: Scanner<'a>,
}

impl<'a> Parser<'a> {
    pub fn new(s: &'a str) -> Parser {
        Parser {
            scanner: Scanner::new(s),
        }
    }

    pub fn new_from_file(s: &str, filename: Option<PathBuf>) -> Parser {
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
            self.scanner.filename.as_ref(),
            pos,
            msg,
            want,
            got,
        )
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
            range: self.scanner.range_from(start),
            segments,
        })
    }

    pub fn parse_commodity(&self) -> Result<Commodity> {
        self.scanner
            .read_identifier()
            .map_err(|e| e.update("parsing commodity"))
            .map(|range| Commodity {
                range,
            })
    }

    pub fn parse_date(&self) -> Result<Date> {
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
        Ok(Date {
            range: self.scanner.range_from(start),
        })
    }

    pub fn parse_decimal(&self) -> Result<Decimal> {
        let start = self.scanner.pos();
        if let Some('-') = self.scanner.current() {
            self.scanner.read_char('-')?;
        }
        self.scanner.read_while_1(Token::Digit, |c| c.is_ascii_digit())?;
        if let Some('.') = self.scanner.current() {
            self.scanner.read_char('.')?;
            self.scanner.read_while_1(Token::Digit, |c| c.is_ascii_digit())?;
        }
        Ok(Decimal {
            range: self.scanner.range_from(start),
        })
    }

    pub fn parse_directive(&self) -> Result<Directive> {
        let start = self.scanner.pos();
        let d = self.parse_date().map_err(|e| e.update("parsing date"))?;
        self.scanner.read_space1()?;
        let c = match self.scanner.current() {
            Some('o') => self.parse_open().map(Command::Open).map_err(|e| {
                self.error(
                    start,
                    Some("parsing 'open' directive".into()),
                    Token::Custom("directive".into()),
                    Token::Error(Box::new(e)),
                )
            })?,
            Some('c') => {
                self.parse_close().map(Command::Close).map_err(|e| {
                    self.error(
                        start,
                        Some("parsing 'close' directive".into()),
                        Token::Custom("directive".into()),
                        Token::Error(Box::new(e)),
                    )
                })?
            }
            o => {
                return Err(self.error(
                    self.scanner.pos(),
                    None,
                    Token::Either(vec![
                        Token::Custom("open".into()),
                        Token::Custom("close".into()),
                    ]),
                    o.map(Token::Char).unwrap_or(Token::EOF),
                ))
            }
        };
        Ok(Directive {
            range: self.scanner.range_from(start),
            date: d,
            command: c,
        })
    }

    pub fn parse_open(&self) -> Result<Open> {
        let start = self.scanner.pos();
        self.scanner.read_string("open")?;
        self.scanner.read_space1()?;
        let a = self
            .parse_account()
            .map_err(|e| e.update("parsing account").into())?;
        self.scanner.read_rest_of_line()?;
        Ok(Open {
            range: self.scanner.range_from(start),
            account: a,
        })
    }

    pub fn parse_close(&self) -> Result<Close> {
        let start = self.scanner.pos();
        self.scanner.read_string("close")?;
        self.scanner.read_space1()?;
        let a = self
            .parse_account()
            .map_err(|e| e.update("parsing account").into())?;
        self.scanner.read_rest_of_line()?;
        Ok(Close {
            range: self.scanner.range_from(start),
            account: a,
        })
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use crate::syntax::scanner::Range;

    use super::*;

    #[test]
    fn test_parse_commodity() {
        assert_eq!(
            Parser::new("USD").parse_commodity().unwrap(),
            Commodity {
                range: Range::new(0, "USD"),
            },
        );
        assert_eq!(
            Parser::new("1FOO  ").parse_commodity().unwrap(),
            Commodity {
                range: Range::new(0, "1FOO"),
            },
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
                range: Range::new(0, "Sometype"),
                segments: vec![Range::new(0, "Sometype")],
            }),
            Parser::new("Sometype").parse_account(),
        );
        assert_eq!(
            Ok(Account {
                range: Range::new(0, "Liabilities:Debt"),
                segments: vec![
                    Range::new(0, "Liabilities"),
                    Range::new(12, "Debt")
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
            Date {
                range: Range::new(0, "0202-02-02"),
            },
            Parser::new("0202-02-02").parse_date().unwrap(),
        );
        assert_eq!(
            Date {
                range: Range::new(0, "2024-02-02"),
            },
            Parser::new("2024-02-02").parse_date().unwrap(),
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
    fn test_parse_decimal() {
        assert_eq!(
            Ok(Decimal {
                range: Range::new(0, "0"),
            }),
            Parser::new("0").parse_decimal(),
        );
        assert_eq!(
            Ok(Decimal {
                range: Range::new(0, "10.01"),
            }),
            Parser::new("10.01").parse_decimal(),
        );
        assert_eq!(
            Ok(Decimal {
                range: Range::new(0, "-10.01"),
            }),
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

    #[test]
    fn test_parse_open() {
        assert_eq!(
            Ok(Open {
                range: Range::new(0, "open   Assets:Foo"),
                account: Account {
                    range: Range::new(7, "Assets:Foo"),
                    segments: vec![
                        Range::new(7, "Assets"),
                        Range::new(14, "Foo")
                    ]
                }
            }),
            Parser::new("open   Assets:Foo").parse_open()
        )
    }

    #[test]
    fn test_parse_close() {
        assert_eq!(
            Ok(Close {
                range: Range::new(0, "close  Assets:Foo"),
                account: Account {
                    range: Range::new(7, "Assets:Foo"),
                    segments: vec![
                        Range::new(7, "Assets"),
                        Range::new(14, "Foo")
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
        fn parse_open() {
            assert_eq!(
                Ok(Directive {
                    range: Range::new(0, "2024-03-01 open Assets:Foo"),
                    date: Date {
                        range: Range::new(0, "2024-03-01"),
                    },
                    command: Command::Open(Open {
                        range: Range::new(11, "open Assets:Foo"),
                        account: Account {
                            range: Range::new(16, "Assets:Foo"),
                            segments: vec![
                                Range::new(16, "Assets"),
                                Range::new(23, "Foo")
                            ]
                        }
                    }),
                }),
                Parser::new("2024-03-01 open Assets:Foo").parse_directive()
            )
        }
    }

    #[test]
    fn parse_close() {
        assert_eq!(
            Ok(Directive {
                range: Range::new(0, "2024-03-01 close Assets:Foo"),
                date: Date {
                    range: Range::new(0, "2024-03-01"),
                },
                command: Command::Close(Close {
                    range: Range::new(11, "close Assets:Foo"),
                    account: Account {
                        range: Range::new(17, "Assets:Foo"),
                        segments: vec![
                            Range::new(17, "Assets"),
                            Range::new(24, "Foo")
                        ]
                    }
                }),
            }),
            Parser::new("2024-03-01 close Assets:Foo").parse_directive()
        )
    }
}
