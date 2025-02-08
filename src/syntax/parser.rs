use std::ops::Range;

use super::cst::{
    Account, Addon, Assertion, Booking, Character, Close, Commodity, Date, Decimal, Directive,
    Include, Open, Price, QuotedString, Sequence, SubAssertion, SyntaxTree, Token, Transaction,
};
use super::error::SyntaxError;
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
            range: self.parser.scanner.range(self.start),
            want: self.token.clone(),
            source: Some(Box::new(source)),
        }
    }

    fn token_error(&self) -> SyntaxError {
        SyntaxError {
            range: self.parser.scanner.range(self.start),
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

    fn range(&self) -> Range<usize> {
        self.start..self.parser.scanner.pos()
    }
}

impl<'a> Parser<'a> {
    pub fn new(s: &'a str) -> Parser<'a> {
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
            range: scope.range(),
            segments,
        })
    }

    fn parse_account_type(&self) -> Result<Range<usize>> {
        let scope = self.scope(Token::AccountType);
        self.scanner
            .read_while_1(&Character::Alphabetic)
            .and_then(|r| match &self.scanner.source[r.clone()] {
                "Assets" | "Liabilities" | "Expenses" | "Equity" | "Income" => Ok(r.clone()),
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
        Ok(Date(scope.range()))
    }

    fn parse_interval(&self) -> Result<Range<usize>> {
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
        Ok(Decimal(scope.range()))
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
            range: scope.range(),
            content,
        })
    }

    pub fn parse(&self) -> Result<SyntaxTree> {
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
        Ok(SyntaxTree {
            range: file_scope.range(),
            directives,
        })
    }

    fn parse_comment(&self) -> Result<Range<usize>> {
        let scope = self.scope(Token::Comment);
        match self.scanner.current() {
            Some('#') | Some('*') => {
                self.scanner.read_until(&Character::NewLine);
                let range = scope.range();
                self.scanner
                    .read_char(&Character::NewLine)
                    .map_err(|e| scope.error(e))?;
                Ok(range)
            }
            Some('/') => {
                self.scanner.read_string("//").map_err(|e| scope.error(e))?;
                self.scanner.read_until(&Character::NewLine);
                let range = scope.range();
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
                range: scope.range(),
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
            range: scope.range(),
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
            range: scope.range(),
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
            range: scope.range(),
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
            range: scope.range(),
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
            range: scope.range(),
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
            range: scope.range(),
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
            range: scope.range(),
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
            range: scope.range(),
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
            range: scope.range(),
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
            range: scope.range(),
            date,
            account,
        }))
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use crate::syntax::cst::Sequence;

    use super::*;

    #[test]
    fn test_parse_commodity1() {
        let text = "USD";
        assert_eq!(Ok(Commodity(0..3)), Parser::new(text).parse_commodity());
    }

    #[test]
    fn test_parse_commodity2() {
        assert_eq!(Ok(Commodity(0..4)), Parser::new("1FOO").parse_commodity());
    }

    #[test]
    fn test_parse_commodity3() {
        let text = " USD";
        assert_eq!(
            Err(SyntaxError {
                range: 0..1,
                want: Token::Commodity,
                source: Some(Box::new(SyntaxError {
                    range: 0..1,
                    want: Token::Sequence(Sequence::One(Character::AlphaNum)),
                    source: None,
                })),
            }),
            Parser::new(text).parse_commodity()
        );
    }

    #[test]
    fn test_parse_commodity4() {
        assert_eq!(
            Err(SyntaxError {
                range: 0..1,
                want: Token::Commodity,
                source: Some(Box::new(SyntaxError {
                    range: 0..1,
                    want: Token::Sequence(Sequence::One(Character::AlphaNum)),
                    source: None,
                })),
            }),
            Parser::new("/USD").parse_commodity()
        );
    }

    #[test]
    fn test_parse_account() {
        assert_eq!(
            Ok(Account {
                range: 0..6,
                segments: vec![Range { start: 0, end: 6 }],
            }),
            Parser::new("Assets").parse_account(),
        );
    }

    #[test]
    fn test_parse_account2() {
        let f2 = "Liabilities:Debt  ";
        assert_eq!(
            Ok(Account {
                range: 0..16,
                segments: vec![0..11, 12..16],
            }),
            Parser::new(f2).parse_account(),
        );
    }

    #[test]
    fn test_parse_account3() {
        let f3 = " USD";
        assert_eq!(
            Err(SyntaxError {
                range: 0..1,
                want: Token::Sequence(Sequence::One(Character::Alphabetic)),
                source: None,
            }),
            Parser::new(f3).parse_account(),
        );
    }

    #[test]
    fn test_parse_date1() {
        let f = "2024-05-07";
        assert_eq!(Ok(Date(0..10)), Parser::new(f).parse_date(),);
    }

    #[test]
    fn test_parse_date2() {
        let f = "024-02-02";
        assert_eq!(
            Err(SyntaxError {
                range: 0..4,
                want: Token::Date,
                source: Some(Box::new(SyntaxError {
                    range: 0..4,
                    want: Token::Sequence(Sequence::NumberOf(4, Character::Digit)),
                    source: None,
                })),
            }),
            Parser::new(f).parse_date(),
        );
    }

    #[test]
    fn test_parse_date3() {
        let f = "2024-02-0";
        assert_eq!(
            Err(SyntaxError {
                range: 0..9,
                want: Token::Date,
                source: Some(Box::new(SyntaxError {
                    range: 8..9,
                    want: Token::Sequence(Sequence::NumberOf(2, Character::Digit)),
                    source: None,
                })),
            }),
            Parser::new(f).parse_date(),
        );
    }
    #[test]
    fn test_parse_date4() {
        let f = "2024-0--0";
        assert_eq!(
            Err(SyntaxError {
                range: 0..7,
                want: Token::Date,
                source: Some(Box::new(SyntaxError {
                    range: 5..7,
                    want: Token::Sequence(Sequence::NumberOf(2, Character::Digit)),
                    source: None,
                })),
            }),
            Parser::new(f).parse_date()
        )
    }

    #[test]
    fn test_parse_interval() {
        for d in ["daily", "weekly", "monthly", "quarterly", "yearly", "once"] {
            assert_eq!(Ok(d), Parser::new(d).parse_interval().map(|r| &d[r]),);
        }
    }

    #[test]
    fn test_parse_decimal() {
        for d in ["0", "10.01", "-10.01"] {
            assert_eq!(
                Ok(Decimal(0..d.len())),
                Parser::new(d).parse_decimal(Token::Decimal),
            );
        }
    }
    #[test]
    fn test_parse_decimal2() {
        let f = "foo";
        assert_eq!(
            Err(SyntaxError {
                range: 0..1,
                want: Token::Decimal,
                source: Some(Box::new(SyntaxError {
                    range: 0..1,
                    want: Token::Sequence(Sequence::One(Character::Digit)),
                    source: None,
                })),
            }),
            Parser::new(f).parse_decimal(Token::Decimal),
        );
    }

    mod addon {
        use crate::syntax::cst::{Account, Addon, Commodity, Date};
        use crate::syntax::parser::Parser;
        use pretty_assertions::assert_eq;

        #[test]
        fn performance() {
            let f1 = "@performance( USD  , VT)";
            assert_eq!(
                Ok(Addon::Performance {
                    range: 0..24,
                    commodities: vec![Commodity(14..17), Commodity(21..23),]
                }),
                Parser::new(f1).parse_addon()
            );
            let f2 = "@performance(  )";
            assert_eq!(
                Ok(Addon::Performance {
                    range: 0..16,
                    commodities: vec![]
                }),
                Parser::new(f2).parse_addon(),
            )
        }

        #[test]
        fn accrual() {
            let f = "@accrue monthly 2024-01-01 2024-12-31 Assets:Payables";
            assert_eq!(
                Ok(Addon::Accrual {
                    range: 0..53,
                    interval: 8..15,
                    start: Date(16..26),
                    end: Date(27..37),
                    account: Account {
                        range: 38..53,
                        segments: vec![38..44, 45..53]
                    }
                }),
                Parser::new(f).parse_addon()
            )
        }
    }

    mod directive {
        use super::*;
        use pretty_assertions::assert_eq;

        #[test]
        fn parse_include() {
            let f = r#"include "/foo/bar/baz/finance.knut""#;
            assert_eq!(
                Ok(Directive::Include(Include {
                    range: 0..35,
                    path: QuotedString {
                        range: 8..35,
                        content: 9..34,
                    }
                })),
                Parser::new(f).parse_directive()
            )
        }

        #[test]
        fn parse_open() {
            let f = "2024-03-01 open Assets:Foo";
            assert_eq!(
                Ok(Directive::Open(Open {
                    range: 0..26,
                    date: Date(0..10),
                    account: Account {
                        range: 16..26,
                        segments: vec![16..22, 23..26]
                    },
                })),
                Parser::new(f).parse_directive()
            )
        }

        #[test]
        fn parse_transaction() {
            let f = "2024-12-31 \"Message\"  \nAssets:Foo Assets:Bar 4.23 USD";
            assert_eq!(
                Ok(Directive::Transaction(Transaction {
                    range: 0..53,
                    addon: None,
                    date: Date(0..10),
                    description: QuotedString {
                        range: 11..20,
                        content: 12..19,
                    },
                    bookings: vec![Booking {
                        range: 23..53,
                        credit: Account {
                            range: 23..33,
                            segments: vec![23..29, 30..33]
                        },
                        debit: Account {
                            range: 34..44,
                            segments: vec![34..40, 41..44]
                        },
                        quantity: Decimal(45..49),
                        commodity: Commodity(50..53),
                    },]
                })),
                Parser::new(f).parse_directive()
            );
        }

        #[test]
        fn parse_close() {
            let f = "2024-03-01 close Assets:Foo";
            assert_eq!(
                Ok(Directive::Close(Close {
                    range: 0..27,
                    date: Date(0..10),
                    account: Account {
                        range: 17..27,
                        segments: vec![17..23, 24..27]
                    }
                })),
                Parser::new(f).parse_directive()
            )
        }

        #[test]
        fn parse_price() {
            let f = "2024-03-01 price FOO 1.543 BAR";
            assert_eq!(
                Ok(Directive::Price(Price {
                    range: 0..30,
                    date: Date(0..10),
                    commodity: Commodity(17..20),
                    price: Decimal(21..26),
                    target: Commodity(27..30),
                })),
                Parser::new(f).parse_directive()
            )
        }

        #[test]
        fn parse_assertion() {
            let f = "2024-03-01 balance Assets:Foo 500.1 BAR";
            assert_eq!(
                Ok(Directive::Assertion(Assertion {
                    range: 0..39,
                    date: Date(0..10),
                    assertions: vec![SubAssertion {
                        range: 19..39,
                        account: Account {
                            range: 19..29,
                            segments: vec![19..25, 26..29],
                        },
                        balance: Decimal(30..35),
                        commodity: Commodity(36..39),
                    }]
                })),
                Parser::new(f).parse_directive()
            )
        }
    }
}
