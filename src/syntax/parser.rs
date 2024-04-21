use crate::syntax::scanner::{Result, Scanner, Token};
use crate::syntax::syntax::Commodity;
use std::path::PathBuf;

use super::scanner::ParserError;

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
        wrapped: ParserError,
    ) -> ParserError {
        ParserError::new(
            &self.scanner.source,
            &self.scanner.filename,
            pos,
            msg,
            want,
            Token::Custom("error".into()),
            Some(wrapped),
        )
    }

    pub fn parse_commodity(&self) -> Result<Commodity> {
        let start = self.scanner.pos();
        self.scanner
            .read_identifier()
            .map(|ident| Commodity { range: ident })
            .map_err(|e| {
                self.error(
                    start,
                    Some("error parsing commodity".into()),
                    Token::Custom("commodity".into()),
                    e,
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_date() {
        assert_eq!(
            Parser::new("USD").parse_commodity().unwrap().range.str,
            "USD"
        );
        assert_eq!(
            Parser::new("1FOO  ").parse_commodity().unwrap().range.str,
            "1FOO"
        );
        assert!(Parser::new(" USD").parse_commodity().is_err());
    }
}
