use crate::syntax::scanner::{Result, Scanner, Token};
use crate::syntax::syntax::Commodity;
use std::path::PathBuf;

use super::scanner::ParserError;
use super::syntax::{Account, Date};

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
}
