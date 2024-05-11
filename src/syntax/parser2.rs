use std::rc::Rc;

use thiserror::Error;

use super::cst::{
    Account, Addon, Assertion, Booking, Commodity, Date, Decimal, Directive, QuotedString, Rng,
    SyntaxTree, Token,
};
use super::file::File;
use super::scanner2::{Character, ScannerError};
use crate::syntax::scanner2::Scanner;

pub struct Parser<'a> {
    scanner: Scanner<'a>,
}

#[derive(Error, Debug, Eq, PartialEq)]
#[error("parser error")]
pub enum ParserError {
    ScannerError {
        file: Rc<File>,
        pos: usize,
        want: Token,
        got: ScannerError,
    },
    ParserError {
        file: Rc<File>,
        pos: usize,
        want: Token,
        got: Box<ParserError>,
    },
    Character {
        file: Rc<File>,
        pos: usize,
        want: Token,
        got: Character,
    },
}

pub type Result<T> = std::result::Result<T, ParserError>;

struct Scope<'a, 'b> {
    parser: &'a Parser<'b>,
    start: usize,
    token: Token,
}

impl<'a, 'b> Scope<'a, 'b> {
    fn error(&self, got: ScannerError) -> ParserError {
        ParserError::ScannerError {
            file: self.parser.scanner.source.clone(),
            pos: self.parser.scanner.pos(),
            want: self.token.clone(),
            got,
        }
    }

    fn error2(&self, got: Character) -> ParserError {
        ParserError::Character {
            file: self.parser.scanner.source.clone(),
            pos: self.parser.scanner.pos(),
            want: self.token.clone(),
            got,
        }
    }

    fn wrap_error(&self, got: ParserError) -> ParserError {
        ParserError::ParserError {
            file: self.parser.scanner.source.clone(),
            pos: self.parser.scanner.pos(),
            want: self.token.clone(),
            got: got.into(),
        }
    }

    fn rng(&self) -> Rng {
        Rng::new(
            self.parser.scanner.source.clone(),
            self.start,
            self.parser.scanner.pos(),
        )
    }
}

impl<'a> Parser<'a> {
    pub fn new(s: &'a Rc<File>) -> Parser<'a> {
        Parser {
            scanner: Scanner::new(s),
        }
    }

    fn scope(&self, token: Token) -> Scope<'_, 'a> {
        Scope {
            parser: self,
            token,
            start: self.scanner.pos(),
        }
    }
    fn parse_account(&self) -> Result<Account> {
        let scope = self.scope(Token::Account);
        let account_type = self
            .scanner
            .read_while_1(Character::AlphaNum)
            .map_err(|e| scope.error(e))?;
        let mut segments = vec![account_type];
        while self.scanner.current() == Some(':') {
            self.scanner
                .read_char(Character::Char(':'))
                .map_err(|e| scope.error(e))?;
            segments.push(
                self.scanner
                    .read_while_1(Character::AlphaNum)
                    .map_err(|e| scope.error(e))?,
            );
        }
        Ok(Account {
            range: scope.rng(),
            segments,
        })
    }

    fn parse_commodity(&self) -> Result<Commodity> {
        let scope = self.scope(Token::Commodity);
        Ok(self
            .scanner
            .read_while_1(Character::AlphaNum)
            .map(Commodity)
            .map_err(|e| scope.error(e))?)
    }

    fn parse_date(&self) -> Result<Date> {
        let scope = self.scope(Token::Date);
        self.scanner
            .read_n(4, Character::Digit)
            .and_then(|_| self.scanner.read_char(Character::Char('-')))
            .and_then(|_| self.scanner.read_n(2, Character::Digit))
            .and_then(|_| self.scanner.read_char(Character::Char('-')))
            .and_then(|_| self.scanner.read_n(2, Character::Digit))
            .map_err(|e| scope.error(e))?;
        Ok(Date(scope.rng()))
    }

    fn parse_interval(&self) -> Result<Rng> {
        let scope = self.scope(Token::Date);
        match self.scanner.current() {
            Some('d') => self
                .scanner
                .read_string("daily")
                .map_err(|e| scope.error(e)),
            Some('w') => self
                .scanner
                .read_string("weekly")
                .map_err(|e| scope.error(e)),
            Some('m') => self
                .scanner
                .read_string("monthly")
                .map_err(|e| scope.error(e)),
            Some('q') => self
                .scanner
                .read_string("quarterly")
                .map_err(|e| scope.error(e)),
            Some('y') => self
                .scanner
                .read_string("yearly")
                .map_err(|e| scope.error(e)),
            Some('o') => self.scanner.read_string("once").map_err(|e| scope.error(e)),
            o => Err(scope.error2(Character::from_char(o))),
        }
    }

    fn parse_decimal(&self) -> Result<Decimal> {
        let scope = self.scope(Token::Decimal);
        if let Some('-') = self.scanner.current() {
            self.scanner
                .read_char(Character::Char('-'))
                .map_err(|e| scope.error(e))?;
        }
        self.scanner
            .read_while_1(Character::Digit)
            .map_err(|e| scope.error(e))?;
        if let Some('.') = self.scanner.current() {
            self.scanner
                .read_char(Character::Char('.'))
                .and_then(|_| self.scanner.read_while_1(Character::Digit))
                .map_err(|e| scope.error(e))?;
        }
        Ok(Decimal(scope.rng()))
    }

    fn parse_quoted_string(&self) -> Result<QuotedString> {
        let scope = self.scope(Token::QuotedString);
        self.scanner
            .read_char(Character::Char('"'))
            .map_err(|e| scope.error(e))?;
        let content = self.scanner.read_while(Character::NotChar('"'));
        self.scanner
            .read_char(Character::Char('"'))
            .map_err(|e| scope.error(e))?;
        Ok(QuotedString {
            range: scope.rng(),
            content,
        })
    }

    pub fn parse(&self) -> Result<SyntaxTree> {
        let scope = self.scope(Token::Either(vec![
            Token::Directive,
            Token::Comment,
            Token::BlankLine,
        ]));
        let mut directives = Vec::new();
        while let Some(c) = self.scanner.current() {
            match c {
                '*' | '/' | '#' => {
                    self.parse_comment()?;
                }
                c if c.is_alphanumeric() || c == '@' => {
                    let d = self.parse_directive().map_err(|e| scope.wrap_error(e))?;
                    directives.push(d)
                }
                c if c.is_whitespace() => {
                    self.scanner
                        .read_rest_of_line()
                        .map_err(|e| scope.error(e))?;
                }
                o => return Err(scope.error2(Character::from_char(Some(o)))),
            }
        }
        Ok(SyntaxTree {
            range: scope.rng(),
            directives,
        })
    }

    fn parse_comment(&self) -> Result<Rng> {
        let scope = self.scope(Token::Comment);
        match self.scanner.current() {
            Some('#') | Some('*') => {
                self.scanner.read_until(Character::NewLine);
                let range = scope.rng();
                self.scanner
                    .read_char(Character::NewLine)
                    .map_err(|e| scope.error(e))?;
                Ok(range)
            }
            Some('/') => {
                self.scanner.read_string("//").map_err(|e| scope.error(e))?;
                self.scanner.read_until(Character::NewLine);
                let range = scope.rng();
                self.scanner
                    .read_char(Character::NewLine)
                    .map_err(|e| scope.error(e))?;
                Ok(range)
            }
            o => Err(scope.error2(Character::from_char(o))),
        }
    }

    fn parse_directive(&self) -> Result<Directive> {
        let scope = self.scope(Token::Directive);
        match self.scanner.current() {
            Some('i') => self.parse_include(&scope),
            Some(c) if c.is_ascii_digit() || c == '@' => self.parse_command(&scope),
            o => Err(scope.error2(Character::from_char(o))),
        }
    }

    fn parse_include(&self, scope: &Scope) -> Result<Directive> {
        self.scanner
            .read_string("include")
            .and_then(|_| self.scanner.read_space_1())
            .map_err(|e| scope.error(e))?;
        let path = self
            .parse_quoted_string()
            .map_err(|e| scope.wrap_error(e))?;
        Ok(Directive::Include {
            range: scope.rng(),
            path,
        })
    }

    fn parse_command(&self, scope: &Scope) -> Result<Directive> {
        let mut addon = None;
        if let Some('@') = self.scanner.current() {
            addon = Some(self.parse_addon()?);
            self.scanner
                .read_rest_of_line()
                .map_err(|e| scope.error(e))?;
        }
        let date = self.parse_date()?;
        self.scanner.read_space_1().map_err(|e| scope.error(e))?;

        let command = match self.scanner.current() {
            Some('p') => self.parse_price(scope, date)?,
            Some('o') => self.parse_open(scope, date)?,
            Some('"') => self.parse_transaction(scope, addon, date)?,
            Some('b') => self.parse_assertion(scope, date)?,
            Some('c') => self.parse_close(scope, date)?,
            o => Err(scope.error2(Character::from_char(o)))?,
        };
        self.scanner
            .read_rest_of_line()
            .map_err(|e| scope.error(e))?;
        Ok(command)
    }

    fn parse_addon(&self) -> Result<Addon> {
        let scope = self.scope(Token::Addon);
        self.scanner
            .read_char(Character::Char('@'))
            .map_err(|e| scope.error(e))?;
        match self.scanner.current() {
            Some('p') => self.parse_performance(&scope),
            Some('a') => self.parse_accrual(&scope),
            o => Err(scope.error2(Character::from_char(o)))?,
        }
    }

    fn parse_performance(&self, original_scope: &Scope) -> Result<Addon> {
        let scope = self.scope(Token::Performance);
        self.scanner
            .read_string("performance")
            .map_err(|e| scope.error(e))?;
        self.scanner.read_space();
        self.scanner
            .read_char(Character::Char('('))
            .map_err(|e| scope.error(e))?;
        self.scanner.read_space();
        let mut commodities = Vec::new();
        while self.scanner.current().map_or(false, char::is_alphanumeric) {
            commodities.push(self.parse_commodity().map_err(|e| scope.wrap_error(e))?);
            self.scanner.read_space();
            if let Some(',') = self.scanner.current() {
                self.scanner
                    .read_char(Character::Char(','))
                    .map_err(|e| scope.error(e))?;
                self.scanner.read_space();
            }
        }
        self.scanner
            .read_char(Character::Char(')'))
            .map_err(|e| scope.error(e))?;
        Ok(Addon::Performance {
            range: original_scope.rng(),
            commodities,
        })
    }

    fn parse_accrual(&self, original_scope: &Scope) -> Result<Addon> {
        let scope = self.scope(Token::Accrual);
        self.scanner
            .read_string("accrue")
            .map_err(|e| scope.error(e))?;
        self.scanner.read_space_1().map_err(|e| scope.error(e))?;
        let interval = self.parse_interval()?;
        self.scanner.read_space_1().map_err(|e| scope.error(e))?;
        let start_date = self.parse_date().map_err(|e| scope.wrap_error(e))?;
        self.scanner.read_space_1().map_err(|e| scope.error(e))?;
        let end_date = self.parse_date().map_err(|e| scope.wrap_error(e))?;
        self.scanner.read_space_1().map_err(|e| scope.error(e))?;
        let account = self.parse_account().map_err(|e| scope.wrap_error(e))?;
        Ok(Addon::Accrual {
            range: original_scope.rng(),
            interval,
            start: start_date,
            end: end_date,
            account,
        })
    }

    fn parse_price(&self, original_scope: &Scope, date: Date) -> Result<Directive> {
        let scope = self.scope(Token::Price);
        self.scanner
            .read_string("price")
            .and_then(|_| self.scanner.read_space_1())
            .map_err(|e| scope.error(e))?;
        let commodity = self.parse_commodity().map_err(|e| scope.wrap_error(e))?;
        self.scanner.read_space_1().map_err(|e| scope.error(e))?;
        let price = self.parse_decimal().map_err(|e| scope.wrap_error(e))?;
        self.scanner.read_space_1().map_err(|e| scope.error(e))?;
        let target = self.parse_commodity().map_err(|e| scope.wrap_error(e))?;
        Ok(Directive::Price {
            range: original_scope.rng(),
            date,
            commodity,
            price,
            target,
        })
    }

    fn parse_open(&self, original_scope: &Scope, date: Date) -> Result<Directive> {
        let scope = self.scope(Token::Open);
        self.scanner
            .read_string("open")
            .and_then(|_| self.scanner.read_space_1())
            .map_err(|e| scope.error(e))?;
        let a = self.parse_account().map_err(|e| scope.wrap_error(e))?;
        Ok(Directive::Open {
            range: original_scope.rng(),
            date,
            account: a,
        })
    }

    fn parse_transaction(
        &self,
        original_scope: &Scope,
        addon: Option<Addon>,
        date: Date,
    ) -> Result<Directive> {
        let scope = self.scope(Token::Transaction);
        let description = self.parse_quoted_string()?;
        self.scanner
            .read_rest_of_line()
            .map_err(|e| scope.error(e))?;
        let mut bookings = Vec::new();
        loop {
            bookings.push(self.parse_booking().map_err(|e| scope.wrap_error(e))?);
            self.scanner
                .read_rest_of_line()
                .map_err(|e| scope.error(e))?;
            if !self.scanner.current().map_or(false, char::is_alphanumeric) {
                break;
            }
        }
        Ok(Directive::Transaction {
            range: original_scope.rng(),
            addon,
            date,
            description,
            bookings,
        })
    }

    pub fn parse_booking(&self) -> Result<Booking> {
        let scope = self.scope(Token::Booking);
        let credit = self.parse_account().map_err(|e| scope.wrap_error(e))?;
        self.scanner.read_space_1().map_err(|e| scope.error(e))?;
        let debit = self.parse_account().map_err(|e| scope.wrap_error(e))?;
        self.scanner.read_space_1().map_err(|e| scope.error(e))?;
        let quantity = self.parse_decimal().map_err(|e| scope.wrap_error(e))?;
        self.scanner.read_space_1().map_err(|e| scope.error(e))?;
        let commodity = self.parse_commodity().map_err(|e| scope.wrap_error(e))?;
        Ok(Booking {
            range: scope.rng(),
            credit,
            debit,
            quantity,
            commodity,
        })
    }

    fn parse_assertion(&self, original_scope: &Scope, date: Date) -> Result<Directive> {
        let scope = self.scope(Token::Assertion);
        self.scanner
            .read_string("balance")
            .and_then(|_| self.scanner.read_space_1())
            .map_err(|e| scope.error(e))?;
        let mut assertions = Vec::new();
        if let Some('\n') = self.scanner.current() {
            self.scanner
                .read_rest_of_line()
                .map_err(|e| scope.error(e))?;
            loop {
                assertions.push(
                    self.parse_sub_assertion()
                        .map_err(|e| scope.wrap_error(e))?,
                );
                self.scanner
                    .read_rest_of_line()
                    .map_err(|e| scope.error(e))?;
                if !Character::AlphaNum.is(self.scanner.current()) {
                    break;
                }
            }
        } else {
            assertions.push(
                self.parse_sub_assertion()
                    .map_err(|e| scope.wrap_error(e))?,
            );
        }
        Ok(Directive::Assertion {
            range: original_scope.rng(),
            date,
            assertions,
        })
    }

    pub fn parse_sub_assertion(&self) -> Result<Assertion> {
        let scope = self.scope(Token::SubAssertion);
        let account = self.parse_account().map_err(|e| scope.wrap_error(e))?;
        self.scanner.read_space_1().map_err(|e| scope.error(e))?;
        let amount = self.parse_decimal().map_err(|e| scope.wrap_error(e))?;
        self.scanner.read_space_1().map_err(|e| scope.error(e))?;
        let commodity = self.parse_commodity().map_err(|e| scope.wrap_error(e))?;
        Ok(Assertion {
            range: scope.rng(),
            account,
            balance: amount,
            commodity,
        })
    }

    fn parse_close(&self, original_scope: &Scope, date: Date) -> Result<Directive> {
        let scope = self.scope(Token::Close);
        self.scanner
            .read_string("close")
            .and_then(|_| self.scanner.read_space_1())
            .map_err(|e| scope.error(e))?;
        let account = self.parse_account().map_err(|e| scope.wrap_error(e))?;
        Ok(Directive::Close {
            range: original_scope.rng(),
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
    fn test_parse_commodity1() {
        let f1 = File::mem("USD");
        assert_eq!(
            Ok(Commodity(Rng::new(f1.clone(), 0, 3))),
            Parser::new(&f1).parse_commodity(),
        );
    }

    #[test]
    fn test_parse_commodity2() {
        let f2 = File::mem("1FOO");
        assert_eq!(
            Ok(Commodity(Rng::new(f2.clone(), 0, 4))),
            Parser::new(&File::mem("1FOO")).parse_commodity()
        );
    }

    #[test]
    fn test_parse_commodity3() {
        let f3 = File::mem(" USD");
        assert_eq!(
            Err(ParserError::ScannerError {
                file: f3.clone(),
                pos: 0,
                want: Token::Commodity,
                got: ScannerError {
                    file: f3.clone(),
                    pos: 0,
                    want: Character::AlphaNum,
                    got: Character::HorizontalSpace,
                },
            }),
            Parser::new(&f3).parse_commodity()
        );
    }

    #[test]
    fn test_parse_commodity4() {
        let f4 = File::mem("/USD");
        assert_eq!(
            Err(ParserError::ScannerError {
                file: f4.clone(),
                pos: 0,
                want: Token::Commodity,
                got: ScannerError {
                    file: f4.clone(),
                    pos: 0,
                    want: Character::AlphaNum,
                    got: Character::Char('/'),
                },
            }),
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
            Err(ParserError::ScannerError {
                file: f3.clone(),
                pos: 0,
                want: Token::Account,
                got: ScannerError {
                    file: f3.clone(),
                    pos: 0,
                    want: Character::AlphaNum,
                    got: Character::HorizontalSpace,
                },
            }),
            Parser::new(&f3).parse_account(),
        );
    }

    #[test]
    fn test_parse_date1() {
        let f = File::mem("2024-05-07");
        assert_eq!(
            Ok(Date(Rng::new(f.clone(), 0, 10))),
            Parser::new(&f).parse_date(),
        );
    }

    #[test]
    fn test_parse_date2() {
        let f = File::mem("024-02-02");
        assert_eq!(
            Err(ParserError::ScannerError {
                file: f.clone(),
                pos: 3,
                want: Token::Date,
                got: ScannerError {
                    file: f.clone(),
                    pos: 3,
                    want: Character::Digit,
                    got: Character::Char('-'),
                },
            }),
            Parser::new(&f).parse_date(),
        );
    }

    #[test]
    fn test_parse_date3() {
        let f = File::mem("2024-02-0");
        assert_eq!(
            Err(ParserError::ScannerError {
                file: f.clone(),
                pos: 9,
                want: Token::Date,
                got: ScannerError {
                    file: f.clone(),
                    pos: 9,
                    want: Character::Digit,
                    got: Character::EOF,
                },
            }),
            Parser::new(&f).parse_date(),
        );
    }
    #[test]
    fn test_parse_date4() {
        let f = File::mem("2024-0--0");
        assert_eq!(
            Err(ParserError::ScannerError {
                file: f.clone(),
                pos: 6,
                want: Token::Date,
                got: ScannerError {
                    file: f.clone(),
                    pos: 6,
                    want: Character::Digit,
                    got: Character::Char('-'),
                },
            }),
            Parser::new(&f).parse_date()
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
    }
    #[test]
    fn test_parse_decimal2() {
        let f = File::mem("foo");
        assert_eq!(
            Err(ParserError::ScannerError {
                file: f.clone(),
                pos: 0,
                want: Token::Decimal,
                got: ScannerError {
                    file: f.clone(),
                    pos: 0,
                    want: Character::Digit,
                    got: Character::Char('f'),
                },
            }),
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
