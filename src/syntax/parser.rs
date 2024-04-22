use crate::syntax::scanner::{Result, Scanner, Token};
use crate::syntax::syntax::Commodity;
use std::path::PathBuf;

use super::scanner::ParserError;
use super::syntax::Date;

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
        wrapped: Option<ParserError>,
    ) -> ParserError {
        ParserError::new(
            &self.scanner.source,
            &self.scanner.filename,
            pos,
            msg,
            want,
            Token::Custom("error".into()),
            wrapped,
        )
    }

    pub fn parse_commodity(&self) -> Result<Commodity> {
        let start = self.scanner.pos();
        self.scanner
            .read_identifier()
            .map(|range| Commodity { range })
            .map_err(|e| {
                self.error(
                    start,
                    Some("error parsing commodity".into()),
                    Token::Custom("commodity".into()),
                    e.into(),
                )
            })
    }

    pub fn parse_date(&self) -> Result<Date> {
        let start = self.scanner.pos();
        for _ in 0..4 {
            self.scanner
                .read_1_with(Token::Digit, |c| c.is_ascii_digit())
                .map_err(|e| self.error(start, None, Token::Custom("date".into()), Some(e)))?;
        }
        for _ in 0..2 {
            self.scanner
                .read_char('-')
                .map_err(|e| self.error(start, None, Token::Custom("date".into()), Some(e)))?;
            for _ in 0..2 {
                self.scanner
                    .read_1_with(Token::Digit, |c| c.is_ascii_digit())
                    .map_err(|e| self.error(start, None, Token::Custom("date".into()), Some(e)))?;
            }
        }
        Ok(Date {
            range: self.scanner.range_from(start),
        })
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_parse_commodity() {
        assert_eq!(
            Parser::new("USD").parse_commodity().unwrap().range.str,
            "USD"
        );
        assert_eq!(
            Parser::new("1FOO  ").parse_commodity().unwrap().range.str,
            "1FOO"
        );
        assert!(Parser::new(" USD").parse_commodity().is_err());
        assert!(Parser::new("/USD").parse_commodity().is_err());
    }

    #[test]
    fn test_parse_date() {
        assert_eq!(
            Parser::new("0202-02-02").parse_date().unwrap().range.str,
            "0202-02-02"
        );
        assert_eq!(
            Parser::new("2024-02-02").parse_date().unwrap().range.str,
            "2024-02-02"
        );
        assert!(Parser::new("024-02-02").parse_date().is_err());
        assert!(Parser::new("2024-02-0").parse_date().is_err());
        assert!(Parser::new("2024-0--0").parse_date().is_err())
    }
}
