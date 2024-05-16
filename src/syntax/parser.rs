use std::rc::Rc;

use super::cst::{
    Account, Addon, Assertion, Booking, Character, Close, Commodity, Date, Decimal, Directive,
    Include, Open, Price, QuotedString, Rng, Sequence, SubAssertion, SyntaxFile, Token,
    Transaction,
};
use super::error::SyntaxError;
use super::file::File;
use crate::syntax::scanner::Scanner;

pub struct Parser<'a> {
    scanner: Scanner<'a>,
}

pub type Result<T> = std::result::Result<T, SyntaxError>;

struct Scope<'a, 'b> {
    parser: &'a Parser<'b>,
    start: usize,
    token: Token,
}

impl<'a, 'b> Scope<'a, 'b> {
    fn error(&self, source: SyntaxError) -> SyntaxError {
        SyntaxError {
            rng: self.parser.scanner.rng(self.start),
            want: self.token.clone(),
            source: Some(Box::new(source)),
        }
    }

    fn token_error(&self) -> SyntaxError {
        SyntaxError {
            rng: self.parser.scanner.rng(self.start),
            want: self.token.clone(),
            source: None,
        }
    }

    fn with(&self, token: Token) -> Scope<'a, 'b> {
        Scope {
            parser: self.parser,
            start: self.start,
            token,
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
        let account_type = self.parse_account_type()?;
        let mut segments = vec![account_type];
        while self.scanner.current() == Some(':') {
            self.scanner
                .read_char(&Character::Char(':'))
                .map_err(|e| scope.error(e))?;
            segments.push(
                self.scanner
                    .read_while_1(&Character::AlphaNum)
                    .map_err(|e| scope.error(e))?,
            );
        }
        Ok(Account {
            range: scope.rng(),
            segments,
        })
    }

    fn parse_account_type(&self) -> Result<Rng> {
        let scope = self.scope(Token::AccountType);
        self.scanner
            .read_while_1(&Character::Alphabetic)
            .and_then(|rng| match rng.text() {
                "Assets" | "Liabilities" | "Expenses" | "Equity" | "Income" => Ok(rng),
                _ => Err(scope.token_error()),
            })
    }

    fn parse_commodity(&self) -> Result<Commodity> {
        let scope = self.scope(Token::Commodity);
        self.scanner
            .read_while_1(&Character::AlphaNum)
            .map(Commodity)
            .map_err(|e| scope.error(e))
    }

    fn parse_date(&self) -> Result<Date> {
        let scope = self.scope(Token::Date);
        self.scanner
            .read_sequence(&Sequence::NumberOf(4, Character::Digit))
            .and_then(|_| self.scanner.read_char(&Character::Char('-')))
            .and_then(|_| {
                self.scanner
                    .read_sequence(&Sequence::NumberOf(2, Character::Digit))
            })
            .and_then(|_| self.scanner.read_char(&Character::Char('-')))
            .and_then(|_| {
                self.scanner
                    .read_sequence(&Sequence::NumberOf(2, Character::Digit))
            })
            .map_err(|e| scope.error(e))?;
        Ok(Date(scope.rng()))
    }

    fn parse_interval(&self) -> Result<Rng> {
        let scope = self.scope(Token::Interval);
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
            _o => Err(scope.token_error()),
        }
    }

    fn parse_decimal(&self, token: Token) -> Result<Decimal> {
        let scope = self.scope(token);
        if let Some('-') = self.scanner.current() {
            self.scanner
                .read_char(&Character::Char('-'))
                .map_err(|e| scope.error(e))?;
        }
        self.scanner
            .read_while_1(&Character::Digit)
            .map_err(|e| scope.error(e))?;
        if let Some('.') = self.scanner.current() {
            self.scanner
                .read_char(&Character::Char('.'))
                .and_then(|_| self.scanner.read_while_1(&Character::Digit))
                .map_err(|e| scope.error(e))?;
        }
        Ok(Decimal(scope.rng()))
    }

    fn parse_quoted_string(&self) -> Result<QuotedString> {
        let scope = self.scope(Token::QuotedString);
        self.scanner
            .read_char(&Character::Char('"'))
            .map_err(|e| scope.error(e))?;
        let content = self.scanner.read_while(&Character::NotChar('"'));
        self.scanner
            .read_char(&Character::Char('"'))
            .map_err(|e| scope.error(e))?;
        Ok(QuotedString {
            range: scope.rng(),
            content,
        })
    }

    pub fn parse(&self) -> Result<SyntaxFile> {
        let file_scope = self.scope(Token::File);
        let mut directives = Vec::new();
        while let Some(c) = self.scanner.current() {
            match c {
                '*' | '/' | '#' => {
                    self.parse_comment()?;
                }
                c if c.is_ascii_digit() || c == 'i' || c == '@' => {
                    let d = self.parse_directive()?;
                    directives.push(d)
                }
                c if c.is_whitespace() => {
                    self.scanner.read_rest_of_line()?;
                }
                _ => {
                    let scope = self.scope(Token::Either(vec![
                        Token::Date,
                        Token::Include,
                        Token::Addon,
                        Token::BlankLine,
                    ]));
                    self.scanner.advance();
                    return Err(scope.token_error());
                }
            }
        }
        Ok(SyntaxFile {
            range: file_scope.rng(),
            directives,
        })
    }

    fn parse_comment(&self) -> Result<Rng> {
        let scope = self.scope(Token::Comment);
        match self.scanner.current() {
            Some('#') | Some('*') => {
                self.scanner.read_until(&Character::NewLine);
                let range = scope.rng();
                self.scanner
                    .read_char(&Character::NewLine)
                    .map_err(|e| scope.error(e))?;
                Ok(range)
            }
            Some('/') => {
                self.scanner.read_string("//").map_err(|e| scope.error(e))?;
                self.scanner.read_until(&Character::NewLine);
                let range = scope.rng();
                self.scanner
                    .read_char(&Character::NewLine)
                    .map_err(|e| scope.error(e))?;
                Ok(range)
            }
            _o => Err(scope.token_error()),
        }
    }

    fn parse_directive(&self) -> Result<Directive> {
        let scope = self.scope(Token::Directive);
        match self.scanner.current() {
            Some('i') => self.parse_include(&scope.with(Token::Include)),
            Some(c) if c.is_ascii_digit() || c == '@' => self.parse_command(&scope),
            _o => Err(SyntaxError {
                want: Token::Directive,
                rng: scope.rng(),
                source: None,
            }),
        }
    }

    fn parse_include(&self, scope: &Scope) -> Result<Directive> {
        self.scanner
            .read_string("include")
            .and_then(|_| self.scanner.read_space_1())
            .map_err(|e| scope.error(e))?;
        let path = self.parse_quoted_string().map_err(|e| scope.error(e))?;
        Ok(Directive::Include(Include {
            range: scope.rng(),
            path,
        }))
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
            Some('p') => self.parse_price(&scope.with(Token::Price), date)?,
            Some('o') => self.parse_open(&scope.with(Token::Open), date)?,
            Some('"') => self.parse_transaction(&scope.with(Token::Transaction), addon, date)?,
            Some('b') => self.parse_assertion(&scope.with(Token::Assertion), date)?,
            Some('c') => self.parse_close(&scope.with(Token::Close), date)?,
            _o => Err(scope.token_error())?,
        };
        self.scanner
            .read_rest_of_line()
            .map_err(|e| scope.error(e))?;
        Ok(command)
    }

    fn parse_addon(&self) -> Result<Addon> {
        let scope = self.scope(Token::Addon);
        self.scanner
            .read_char(&Character::Char('@'))
            .map_err(|e| scope.error(e))?;
        match self.scanner.current() {
            Some('p') => self.parse_performance(&scope.with(Token::Performance)),
            Some('a') => self.parse_accrual(&scope.with(Token::Accrual)),
            _o => Err(scope.token_error())?,
        }
    }

    fn parse_performance(&self, scope: &Scope) -> Result<Addon> {
        self.scanner
            .read_string("performance")
            .map_err(|e| scope.error(e))?;
        self.scanner.read_space();
        self.scanner
            .read_char(&Character::Char('('))
            .map_err(|e| scope.error(e))?;
        self.scanner.read_space();
        let mut commodities = Vec::new();
        while self.scanner.current().map_or(false, char::is_alphanumeric) {
            commodities.push(self.parse_commodity().map_err(|e| scope.error(e))?);
            self.scanner.read_space();
            if let Some(',') = self.scanner.current() {
                self.scanner
                    .read_char(&Character::Char(','))
                    .map_err(|e| scope.error(e))?;
                self.scanner.read_space();
            }
        }
        self.scanner
            .read_char(&Character::Char(')'))
            .map_err(|e| scope.error(e))?;
        Ok(Addon::Performance {
            range: scope.rng(),
            commodities,
        })
    }

    fn parse_accrual(&self, scope: &Scope) -> Result<Addon> {
        self.scanner
            .read_string("accrue")
            .map_err(|e| scope.error(e))?;
        self.scanner.read_space_1().map_err(|e| scope.error(e))?;
        let interval = self.parse_interval()?;
        self.scanner.read_space_1().map_err(|e| scope.error(e))?;
        let start_date = self.parse_date().map_err(|e| scope.error(e))?;
        self.scanner.read_space_1().map_err(|e| scope.error(e))?;
        let end_date = self.parse_date().map_err(|e| scope.error(e))?;
        self.scanner.read_space_1().map_err(|e| scope.error(e))?;
        let account = self.parse_account().map_err(|e| scope.error(e))?;
        Ok(Addon::Accrual {
            range: scope.rng(),
            interval,
            start: start_date,
            end: end_date,
            account,
        })
    }

    fn parse_price(&self, scope: &Scope, date: Date) -> Result<Directive> {
        self.scanner
            .read_string("price")
            .and_then(|_| self.scanner.read_space_1())
            .map_err(|e| scope.error(e))?;
        let commodity = self.parse_commodity().map_err(|e| scope.error(e))?;
        self.scanner.read_space_1().map_err(|e| scope.error(e))?;
        let price = self
            .parse_decimal(Token::Price)
            .map_err(|e| scope.error(e))?;
        self.scanner.read_space_1().map_err(|e| scope.error(e))?;
        let target = self.parse_commodity().map_err(|e| scope.error(e))?;
        Ok(Directive::Price(Price {
            range: scope.rng(),
            date,
            commodity,
            price,
            target,
        }))
    }

    fn parse_open(&self, scope: &Scope, date: Date) -> Result<Directive> {
        self.scanner
            .read_string("open")
            .and_then(|_| self.scanner.read_space_1())
            .map_err(|e| scope.error(e))?;
        let a = self.parse_account().map_err(|e| scope.error(e))?;
        Ok(Directive::Open(Open {
            range: scope.rng(),
            date,
            account: a,
        }))
    }

    fn parse_transaction(
        &self,
        scope: &Scope,
        addon: Option<Addon>,
        date: Date,
    ) -> Result<Directive> {
        let description = self.parse_quoted_string()?;
        self.scanner
            .read_rest_of_line()
            .map_err(|e| scope.error(e))?;
        let mut bookings = Vec::new();
        loop {
            bookings.push(self.parse_booking().map_err(|e| scope.error(e))?);
            self.scanner
                .read_rest_of_line()
                .map_err(|e| scope.error(e))?;
            if !self.scanner.current().map_or(false, char::is_alphanumeric) {
                break;
            }
        }
        Ok(Directive::Transaction(Transaction {
            range: scope.rng(),
            addon,
            date,
            description,
            bookings,
        }))
    }

    pub fn parse_booking(&self) -> Result<Booking> {
        let scope = self.scope(Token::Booking);
        let credit = self.parse_account().map_err(|e| scope.error(e))?;
        self.scanner.read_space_1().map_err(|e| scope.error(e))?;
        let debit = self.parse_account().map_err(|e| scope.error(e))?;
        self.scanner.read_space_1().map_err(|e| scope.error(e))?;
        let quantity = self
            .parse_decimal(Token::Quantity)
            .map_err(|e| scope.error(e))?;
        self.scanner.read_space_1().map_err(|e| scope.error(e))?;
        let commodity = self.parse_commodity().map_err(|e| scope.error(e))?;
        Ok(Booking {
            range: scope.rng(),
            credit,
            debit,
            quantity,
            commodity,
        })
    }

    fn parse_assertion(&self, scope: &Scope, date: Date) -> Result<Directive> {
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
                assertions.push(self.parse_sub_assertion().map_err(|e| scope.error(e))?);
                self.scanner
                    .read_rest_of_line()
                    .map_err(|e| scope.error(e))?;
                if !Character::AlphaNum.is(self.scanner.current()) {
                    break;
                }
            }
        } else {
            assertions.push(self.parse_sub_assertion().map_err(|e| scope.error(e))?);
        }
        Ok(Directive::Assertion(Assertion {
            range: scope.rng(),
            date,
            assertions,
        }))
    }

    pub fn parse_sub_assertion(&self) -> Result<SubAssertion> {
        let scope = self.scope(Token::SubAssertion);
        let account = self.parse_account().map_err(|e| scope.error(e))?;
        self.scanner.read_space_1().map_err(|e| scope.error(e))?;
        let amount = self
            .parse_decimal(Token::Quantity)
            .map_err(|e| scope.error(e))?;
        self.scanner.read_space_1().map_err(|e| scope.error(e))?;
        let commodity = self.parse_commodity().map_err(|e| scope.error(e))?;
        Ok(SubAssertion {
            range: scope.rng(),
            account,
            balance: amount,
            commodity,
        })
    }

    fn parse_close(&self, scope: &Scope, date: Date) -> Result<Directive> {
        self.scanner
            .read_string("close")
            .and_then(|_| self.scanner.read_space_1())
            .map_err(|e| scope.error(e))?;
        let account = self.parse_account().map_err(|e| scope.error(e))?;
        Ok(Directive::Close(Close {
            range: scope.rng(),
            date,
            account,
        }))
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use crate::syntax::{cst::Rng, cst::Sequence};

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
            Err(SyntaxError {
                rng: Rng::new(f3.clone(), 0, 1),
                want: Token::Commodity,
                source: Some(Box::new(SyntaxError {
                    rng: Rng::new(f3.clone(), 0, 1),
                    want: Token::Sequence(Sequence::One(Character::AlphaNum)),
                    source: None,
                })),
            }),
            Parser::new(&f3).parse_commodity()
        );
    }

    #[test]
    fn test_parse_commodity4() {
        let f4 = File::mem("/USD");
        assert_eq!(
            Err(SyntaxError {
                rng: Rng::new(f4.clone(), 0, 1),
                want: Token::Commodity,
                source: Some(Box::new(SyntaxError {
                    rng: Rng::new(f4.clone(), 0, 1),
                    want: Token::Sequence(Sequence::One(Character::AlphaNum)),
                    source: None,
                })),
            }),
            Parser::new(&f4).parse_commodity()
        );
    }

    #[test]
    fn test_parse_account() {
        let f1 = File::mem("Assets");
        assert_eq!(
            Ok(Account {
                range: Rng::new(f1.clone(), 0, 6),
                segments: vec![Rng::new(f1.clone(), 0, 6)],
            }),
            Parser::new(&f1).parse_account(),
        );
    }

    #[test]
    fn test_parse_account2() {
        let f2 = File::mem("Liabilities:Debt  ");
        assert_eq!(
            Ok(Account {
                range: Rng::new(f2.clone(), 0, 16),
                segments: vec![Rng::new(f2.clone(), 0, 11), Rng::new(f2.clone(), 12, 16)],
            }),
            Parser::new(&f2).parse_account(),
        );
    }

    #[test]
    fn test_parse_account3() {
        let f3 = File::mem(" USD");
        assert_eq!(
            Err(SyntaxError {
                rng: Rng::new(f3.clone(), 0, 1),
                want: Token::Sequence(Sequence::One(Character::Alphabetic)),
                source: None,
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
            Err(SyntaxError {
                rng: Rng::new(f.clone(), 0, 4),
                want: Token::Date,
                source: Some(Box::new(SyntaxError {
                    rng: Rng::new(f.clone(), 0, 4),
                    want: Token::Sequence(Sequence::NumberOf(4, Character::Digit)),
                    source: None,
                })),
            }),
            Parser::new(&f).parse_date(),
        );
    }

    #[test]
    fn test_parse_date3() {
        let f = File::mem("2024-02-0");
        assert_eq!(
            Err(SyntaxError {
                rng: Rng::new(f.clone(), 0, 9),
                want: Token::Date,
                source: Some(Box::new(SyntaxError {
                    rng: Rng::new(f.clone(), 8, 9),
                    want: Token::Sequence(Sequence::NumberOf(2, Character::Digit)),
                    source: None,
                })),
            }),
            Parser::new(&f).parse_date(),
        );
    }
    #[test]
    fn test_parse_date4() {
        let f = File::mem("2024-0--0");
        assert_eq!(
            Err(SyntaxError {
                rng: Rng::new(f.clone(), 0, 7),
                want: Token::Date,
                source: Some(Box::new(SyntaxError {
                    rng: Rng::new(f.clone(), 5, 7),
                    want: Token::Sequence(Sequence::NumberOf(2, Character::Digit)),
                    source: None,
                })),
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
                Parser::new(&f).parse_decimal(Token::Decimal),
            );
        }
    }
    #[test]
    fn test_parse_decimal2() {
        let f = File::mem("foo");
        assert_eq!(
            Err(SyntaxError {
                rng: Rng::new(f.clone(), 0, 1),
                want: Token::Decimal,
                source: Some(Box::new(SyntaxError {
                    rng: Rng::new(f.clone(), 0, 1),
                    want: Token::Sequence(Sequence::One(Character::Digit)),
                    source: None,
                })),
            }),
            Parser::new(&f).parse_decimal(Token::Decimal),
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
                Ok(Directive::Include(Include {
                    range: Rng::new(f.clone(), 0, 35),
                    path: QuotedString {
                        range: Rng::new(f.clone(), 8, 35),
                        content: Rng::new(f.clone(), 9, 34),
                    }
                })),
                Parser::new(&f).parse_directive()
            )
        }

        #[test]
        fn parse_open() {
            let f = File::mem("2024-03-01 open Assets:Foo");
            assert_eq!(
                Ok(Directive::Open(Open {
                    range: Rng::new(f.clone(), 0, 26),
                    date: Date(Rng::new(f.clone(), 0, 10)),
                    account: Account {
                        range: Rng::new(f.clone(), 16, 26),
                        segments: vec![Rng::new(f.clone(), 16, 22), Rng::new(f.clone(), 23, 26)]
                    },
                })),
                Parser::new(&f).parse_directive()
            )
        }

        #[test]
        fn parse_transaction() {
            let f = File::mem("2024-12-31 \"Message\"  \nAssets:Foo Assets:Bar 4.23 USD");
            assert_eq!(
                Ok(Directive::Transaction(Transaction {
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
                })),
                Parser::new(&f).parse_directive()
            );
        }

        #[test]
        fn parse_close() {
            let f = File::mem("2024-03-01 close Assets:Foo");
            assert_eq!(
                Ok(Directive::Close(Close {
                    range: Rng::new(f.clone(), 0, 27),
                    date: Date(Rng::new(f.clone(), 0, 10)),
                    account: Account {
                        range: Rng::new(f.clone(), 17, 27),
                        segments: vec![Rng::new(f.clone(), 17, 23), Rng::new(f.clone(), 24, 27)]
                    }
                })),
                Parser::new(&f).parse_directive()
            )
        }

        #[test]
        fn parse_price() {
            let f = File::mem("2024-03-01 price FOO 1.543 BAR");
            assert_eq!(
                Ok(Directive::Price(Price {
                    range: Rng::new(f.clone(), 0, 30),
                    date: Date(Rng::new(f.clone(), 0, 10)),
                    commodity: Commodity(Rng::new(f.clone(), 17, 20)),
                    price: Decimal(Rng::new(f.clone(), 21, 26)),
                    target: Commodity(Rng::new(f.clone(), 27, 30)),
                })),
                Parser::new(&f).parse_directive()
            )
        }

        #[test]
        fn parse_assertion() {
            let f = File::mem("2024-03-01 balance Assets:Foo 500.1 BAR");
            assert_eq!(
                Ok(Directive::Assertion(Assertion {
                    range: Rng::new(f.clone(), 0, 39),
                    date: Date(Rng::new(f.clone(), 0, 10)),
                    assertions: vec![SubAssertion {
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
                })),
                Parser::new(&f).parse_directive()
            )
        }
    }
}
