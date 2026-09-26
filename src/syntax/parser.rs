use std::ops::Range;

use crate::syntax::cst::VirtualAccount;

use super::cst::{
    Account, Addon, Amount, Arrow, Assertion, Booking, Bookings, Character, Close, Commodity, Date,
    Decimal, Description, Direction, Directive, Group, Include, Leg, Open, Price, QuotedString,
    Sequence, SubAssertion, SyntaxTree, Token, Transaction,
};
use super::error::SyntaxError;
use super::scanner::Scanner;
use super::scope::Scope;

pub type Result<T> = std::result::Result<T, SyntaxError>;

pub struct Parser<'a> {
    scanner: Scanner<'a>,
}

impl<'a> Parser<'a> {
    pub fn new(s: &'a str) -> Parser<'a> {
        Parser {
            scanner: Scanner::new(s),
        }
    }

    fn parse_account(&self) -> Result<Account> {
        let scope = self.scanner.enter(Token::Account);
        let account_type = self.parse_account_type()?;
        let mut segments = vec![account_type];
        while self.scanner.current() == Some(':') {
            self.scanner.read_char(&Character::Char(':'))?;
            segments.push(self.scanner.read_while_1(&Character::AlphaNum)?);
        }
        Ok(Account {
            range: scope.range(),
            segments,
        })
    }

    fn parse_account_type(&self) -> Result<Range<usize>> {
        let scope = self.scanner.enter(Token::AccountType);
        self.scanner
            .read_while_1(&Character::Alphabetic)
            .and_then(|r| match &scope.source()[r.clone()] {
                "Assets" | "Liabilities" | "Expenses" | "Equity" | "Income" => Ok(r.clone()),
                _ => Err(scope.token_error()),
            })
    }

    fn parse_commodity(&self) -> Result<Commodity> {
        let _scope = self.scanner.enter(Token::Commodity);
        self.scanner
            .read_while_1(&Character::AlphaNum)
            .map(Commodity)
    }

    fn parse_date(&self) -> Result<Date> {
        let scope = self.scanner.enter(Token::Date);
        self.scanner
            .read_sequence(&Sequence::NumberOf(4, Character::Digit))?;
        self.scanner.read_char(&Character::Char('-'))?;
        self.scanner
            .read_sequence(&Sequence::NumberOf(2, Character::Digit))?;
        self.scanner.read_char(&Character::Char('-'))?;
        self.scanner
            .read_sequence(&Sequence::NumberOf(2, Character::Digit))?;
        Ok(Date(scope.range()))
    }

    fn parse_interval(&self) -> Result<Range<usize>> {
        let scope = self.scanner.enter(Token::Interval);
        match self.scanner.current() {
            Some('d') => self.scanner.read_string("daily"),
            Some('w') => self.scanner.read_string("weekly"),
            Some('m') => self.scanner.read_string("monthly"),
            Some('q') => self.scanner.read_string("quarterly"),
            Some('y') => self.scanner.read_string("yearly"),
            Some('o') => self.scanner.read_string("once"),
            _o => Err(scope.token_error()),
        }
    }

    fn parse_decimal(&self, token: Token) -> Result<Decimal> {
        let scope = self.scanner.enter(token);
        if let Some('-') = self.scanner.current() {
            self.scanner.read_char(&Character::Char('-'))?;
        }
        self.scanner.read_while_1(&Character::Digit)?;
        if let Some('.') = self.scanner.current() {
            self.scanner.read_char(&Character::Char('.'))?;
            self.scanner.read_while_1(&Character::Digit)?;
        }
        Ok(Decimal(scope.range()))
    }

    fn parse_quoted_string(&self) -> Result<QuotedString> {
        let scope = self.scanner.enter(Token::QuotedString);
        self.scanner.read_char(&Character::Char('"'))?;
        let content = self.scanner.read_while(&Character::NotChar('"'));
        self.scanner.read_char(&Character::Char('"'))?;
        Ok(QuotedString {
            range: scope.range(),
            content,
        })
    }

    pub fn parse(&self) -> Result<SyntaxTree> {
        // A file is not a production in progress: naming it in every error
        // below would say nothing.
        let start = self.scanner.pos();
        let mut directives = Vec::new();
        while let Some(c) = self.scanner.current() {
            match c {
                '*' | '/' | '#' => {
                    self.parse_comment()?;
                }
                c if c.is_ascii_digit() || c == 'i' || c == '@' || c == 'v' => {
                    let d = self.parse_directive()?;
                    directives.push(d)
                }
                c if c.is_whitespace() => {
                    self.scanner.read_rest_of_line()?;
                }
                _ => {
                    let scope = self.scanner.enter(Token::Either(vec![
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
            range: start..self.scanner.pos(),
            directives,
        })
    }

    fn parse_comment(&self) -> Result<Range<usize>> {
        let scope = self.scanner.enter(Token::Comment);
        match self.scanner.current() {
            Some('#') | Some('*') => {
                self.scanner.read_until(&Character::NewLine);
                let range = scope.range();
                self.scanner.read_char(&Character::NewLine)?;
                Ok(range)
            }
            Some('/') => {
                self.scanner.read_string("//")?;
                self.scanner.read_until(&Character::NewLine);
                let range = scope.range();
                self.scanner.read_char(&Character::NewLine)?;
                Ok(range)
            }
            _o => Err(scope.token_error()),
        }
    }

    fn parse_directive(&self) -> Result<Directive> {
        let scope = self.scanner.enter(Token::Directive);
        match self.scanner.current() {
            Some('i') => self.parse_include(&scope),
            Some('v') => self.parse_virtual_account(&scope),
            Some(c) if c.is_ascii_digit() || c == '@' => self.parse_command(&scope),
            _o => Err(scope.token_error()),
        }
    }

    fn parse_include(&self, scope: &Scope) -> Result<Directive> {
        let scope = scope.with(Token::Include);
        self.scanner.read_string("include")?;
        self.scanner.read_space_1()?;
        let path = self.parse_quoted_string()?;
        Ok(Directive::Include(Include {
            range: scope.range(),
            path,
        }))
    }

    fn parse_virtual_account(&self, scope: &Scope) -> Result<Directive> {
        let scope = scope.with(Token::VirtualAccount);
        self.scanner.read_string("virtual")?;
        self.scanner.read_space_1()?;
        let account = self.parse_account()?;
        self.scanner.read_rest_of_line()?;
        let mut patterns = Vec::new();
        while self.scanner.current().is_some_and(char::is_alphanumeric) {
            let pattern = self.scanner.read_until(&Character::WhiteSpace);
            self.scanner.read_rest_of_line()?;
            patterns.push(pattern);
        }
        Ok(Directive::VirtualAccount(VirtualAccount {
            range: scope.range(),
            account,
            patterns,
        }))
    }

    fn parse_command(&self, scope: &Scope) -> Result<Directive> {
        let mut addon = None;
        if let Some('@') = self.scanner.current() {
            addon = Some(self.parse_addon()?);
            self.scanner.read_rest_of_line()?;
        }
        let date = self.parse_date()?;
        self.scanner.read_space_1()?;

        let command = match self.scanner.current() {
            Some('p') => self.parse_price(scope, date)?,
            Some('o') => self.parse_open(scope, date)?,
            Some('"') => self.parse_transaction(scope, addon, date)?,
            Some('b') => self.parse_assertion(scope, date)?,
            Some('c') => self.parse_close(scope, date)?,
            // Nothing else on the date line: the description is on the line
            // below it, and the bookings are written as groups.
            Some('\n') => self.parse_grouped_transaction(scope, addon, date)?,
            _o => Err(scope.token_error())?,
        };
        self.scanner.read_rest_of_line()?;
        Ok(command)
    }

    fn parse_addon(&self) -> Result<Addon> {
        let scope = self.scanner.enter(Token::Addon);
        self.scanner.read_char(&Character::Char('@'))?;
        match self.scanner.current() {
            Some('p') => self.parse_performance(&scope),
            Some('a') => self.parse_accrual(&scope),
            _o => Err(scope.token_error())?,
        }
    }

    fn parse_performance(&self, scope: &Scope) -> Result<Addon> {
        let scope = scope.with(Token::Performance);
        self.scanner.read_string("performance")?;
        self.scanner.read_space();
        self.scanner.read_char(&Character::Char('('))?;
        self.scanner.read_space();
        let mut commodities = Vec::new();
        while self.scanner.current().is_some_and(char::is_alphanumeric) {
            commodities.push(self.parse_commodity()?);
            self.scanner.read_space();
            if let Some(',') = self.scanner.current() {
                self.scanner.read_char(&Character::Char(','))?;
                self.scanner.read_space();
            }
        }
        self.scanner.read_char(&Character::Char(')'))?;
        Ok(Addon::Performance {
            range: scope.range(),
            commodities,
        })
    }

    fn parse_accrual(&self, scope: &Scope) -> Result<Addon> {
        let scope = scope.with(Token::Accrual);
        self.scanner.read_string("accrue")?;
        self.scanner.read_space_1()?;
        let interval = self.parse_interval()?;
        self.scanner.read_space_1()?;
        let start_date = self.parse_date()?;
        self.scanner.read_space_1()?;
        let end_date = self.parse_date()?;
        self.scanner.read_space_1()?;
        let account = self.parse_account()?;
        Ok(Addon::Accrual {
            range: scope.range(),
            interval,
            start: start_date,
            end: end_date,
            account,
        })
    }

    fn parse_price(&self, scope: &Scope, date: Date) -> Result<Directive> {
        let scope = scope.with(Token::Price);
        self.scanner.read_string("price")?;
        self.scanner.read_space_1()?;
        let commodity = self.parse_commodity()?;
        self.scanner.read_space_1()?;
        let price = self.parse_decimal(Token::Price)?;
        self.scanner.read_space_1()?;
        let target = self.parse_commodity()?;
        Ok(Directive::Price(Price {
            range: scope.range(),
            date,
            commodity,
            price,
            target,
        }))
    }

    fn parse_open(&self, scope: &Scope, date: Date) -> Result<Directive> {
        let scope = scope.with(Token::Open);
        self.scanner.read_string("open")?;
        self.scanner.read_space_1()?;
        let a = self.parse_account()?;
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
        let scope = scope.with(Token::Transaction);
        let description = Description::Quoted(self.parse_quoted_string()?);
        self.scanner.read_rest_of_line()?;
        let mut bookings = Vec::new();
        loop {
            bookings.push(self.parse_booking()?);
            self.scanner.read_rest_of_line()?;
            if !self.scanner.current().is_some_and(char::is_alphanumeric) {
                break;
            }
        }
        Ok(Directive::Transaction(Transaction {
            range: scope.range(),
            addon,
            date,
            description,
            bookings: Bookings::Lines(bookings),
        }))
    }

    /// A transaction whose description sits on the indented line below the
    /// date, and whose bookings are written as groups of credit accounts at
    /// column zero and debit accounts marked with `->`.
    fn parse_grouped_transaction(
        &self,
        scope: &Scope,
        addon: Option<Addon>,
        date: Date,
    ) -> Result<Directive> {
        let scope = scope.with(Token::Transaction);
        self.scanner.read_rest_of_line()?;
        let description = self.parse_indented_description()?;
        let mut groups = Vec::new();
        loop {
            groups.push(self.parse_group()?);
            if !Character::Alphabetic.is(self.scanner.current()) {
                break;
            }
        }
        Ok(Directive::Transaction(Transaction {
            range: scope.range(),
            addon,
            date,
            description,
            bookings: Bookings::Groups(groups),
        }))
    }

    /// An unquoted description, on the indented lines below the date line. A
    /// line which is blank, or not indented, belongs to what follows.
    fn parse_indented_description(&self) -> Result<Description> {
        let scope = self.scanner.enter(Token::Description);
        let mut lines = Vec::new();
        loop {
            let rollback = self.scanner.snapshot();
            if !Character::HorizontalSpace.is(self.scanner.current()) {
                break;
            }
            self.scanner.read_space();
            let line = trim_end(scope.source(), self.scanner.read_until(&Character::NewLine));
            if line.is_empty() {
                rollback();
                break;
            }
            self.scanner.read_rest_of_line()?;
            lines.push(line);
        }
        if lines.is_empty() {
            return Err(scope.token_error());
        }
        Ok(Description::Indented(lines))
    }

    /// The accounts of a group at column zero, followed by the accounts
    /// facing them, each marked with an arrow. Exactly one side is a single
    /// account without an amount; every leg on the other side has one.
    fn parse_group(&self) -> Result<Group> {
        let scope = self.scanner.enter(Token::Group);
        let mut accounts = Vec::new();
        loop {
            accounts.push(self.parse_leg()?);
            self.scanner.read_rest_of_line()?;
            if !Character::Alphabetic.is(self.scanner.current()) {
                break;
            }
        }
        let mut arrows = Vec::new();
        loop {
            let direction = match self.scanner.current() {
                Some('-') => Direction::Out,
                Some('<') => Direction::In,
                _ => break,
            };
            self.scanner.read_string(match direction {
                Direction::Out => "->",
                Direction::In => "<-",
            })?;
            self.scanner.read_space_1()?;
            let leg = self.parse_leg()?;
            self.scanner.read_rest_of_line()?;
            arrows.push(Arrow { direction, leg });
        }
        let group = Group {
            range: scope.range(),
            accounts,
            arrows,
        };
        if !well_shaped(&group) {
            return Err(scope.error(Token::Custom(GROUP_SHAPE.to_string())));
        }
        Ok(group)
    }

    /// One side of the bookings of a group: an account, and an amount unless
    /// this is the single account the other side fans out from.
    fn parse_leg(&self) -> Result<Leg> {
        let scope = self.scanner.enter(Token::Booking);
        let account = self.parse_account()?;
        let mut range = scope.range();
        self.scanner.read_space();
        let amount = match self.scanner.current() {
            Some(c) if c.is_ascii_digit() || c == '-' => {
                let quantity = self.parse_decimal(Token::Quantity)?;
                self.scanner.read_space_1()?;
                let commodity = self.parse_commodity()?;
                range = scope.range();
                Some(Amount {
                    quantity,
                    commodity,
                })
            }
            _ => None,
        };
        Ok(Leg {
            range,
            account,
            amount,
        })
    }

    pub fn parse_booking(&self) -> Result<Booking> {
        let scope = self.scanner.enter(Token::Booking);
        let credit = self.parse_account()?;
        self.scanner.read_space_1()?;
        let debit = self.parse_account()?;
        self.scanner.read_space_1()?;
        let quantity = self.parse_decimal(Token::Quantity)?;
        self.scanner.read_space_1()?;
        let commodity = self.parse_commodity()?;
        Ok(Booking {
            range: scope.range(),
            credit,
            debit,
            quantity,
            commodity,
        })
    }

    fn parse_assertion(&self, scope: &Scope, date: Date) -> Result<Directive> {
        let scope = scope.with(Token::Assertion);
        self.scanner.read_string("balance")?;
        self.scanner.read_space_1()?;
        let mut assertions = Vec::new();
        if let Some('\n') = self.scanner.current() {
            self.scanner.read_rest_of_line()?;
            loop {
                assertions.push(self.parse_sub_assertion()?);
                self.scanner.read_rest_of_line()?;
                if !Character::AlphaNum.is(self.scanner.current()) {
                    break;
                }
            }
        } else {
            assertions.push(self.parse_sub_assertion()?);
        }
        Ok(Directive::Assertion(Assertion {
            range: scope.range(),
            date,
            assertions,
        }))
    }

    pub fn parse_sub_assertion(&self) -> Result<SubAssertion> {
        let scope = self.scanner.enter(Token::SubAssertion);
        let account = self.parse_account()?;
        self.scanner.read_space_1()?;
        let amount = self.parse_decimal(Token::Quantity)?;
        self.scanner.read_space_1()?;
        let commodity = self.parse_commodity()?;
        Ok(SubAssertion {
            range: scope.range(),
            account,
            balance: amount,
            commodity,
        })
    }

    fn parse_close(&self, scope: &Scope, date: Date) -> Result<Directive> {
        let scope = scope.with(Token::Close);
        self.scanner.read_string("close")?;
        self.scanner.read_space_1()?;
        let account = self.parse_account()?;
        Ok(Directive::Close(Close {
            range: scope.range(),
            date,
            account,
        }))
    }
}

/// What a group must look like, for the error message when it does not.
const GROUP_SHAPE: &str = "one account without an amount, facing accounts which all have one";

/// Whether the amounts of a group sit on exactly one of its sides, so that it
/// expands to one booking per account on that side.
fn well_shaped(group: &Group) -> bool {
    let fans_out = |single: &[&Leg], many: &[&Leg]| {
        matches!(single, [leg] if leg.amount.is_none())
            && !many.is_empty()
            && many.iter().all(|leg| leg.amount.is_some())
    };
    let accounts = group.accounts.iter().collect::<Vec<_>>();
    let arrows = group.arrows.iter().map(|a| &a.leg).collect::<Vec<_>>();
    fans_out(&accounts, &arrows) || fans_out(&arrows, &accounts)
}

/// The range without the trailing whitespace of the text it covers.
fn trim_end(source: &str, range: Range<usize>) -> Range<usize> {
    range.start..range.start + source[range.clone()].trim_end().len()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use crate::syntax::cst::Sequence;

    use super::*;

    /// Every production takes its frame off the stack again, however it
    /// ends, so a failure names the productions it is in and no others.
    #[test]
    fn the_stack_unwinds() {
        for f in [
            "2024-12-31 open Assets:Foo\n",
            "2024-12-31 opne Assets:Foo\n",
            "2024-12-31 price FOO 1.5 BAR\n\n2024-12-31 close Assets:Foo\n",
            "2024-12-31\n  Buy\n  and more\nAssets:Foo\n-> Assets:Bar 1 CHF\n",
            "2024-12-31\n  Buy\nAssets:Foo 1 CHF\n-> Assets:Bar 2 CHF\n",
            "2024-12-31 \"Buy\"\nAssets:Foo Assets:Bar\n",
            "not a directive\n",
        ] {
            let parser = Parser::new(f);
            let _ = parser.parse();
            assert_eq!(0, parser.scanner.depth(), "{f}");
        }
    }

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
                want: Token::Sequence(Sequence::One(Character::AlphaNum)),
                context: Some(Box::new(SyntaxError {
                    range: 0..1,
                    want: Token::Commodity,
                    context: None,
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
                want: Token::Sequence(Sequence::One(Character::AlphaNum)),
                context: Some(Box::new(SyntaxError {
                    range: 0..1,
                    want: Token::Commodity,
                    context: None,
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
                context: Some(Box::new(SyntaxError {
                    range: 0..1,
                    want: Token::AccountType,
                    context: Some(Box::new(SyntaxError {
                        range: 0..1,
                        want: Token::Account,
                        context: None,
                    })),
                })),
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
                want: Token::Sequence(Sequence::NumberOf(4, Character::Digit)),
                context: Some(Box::new(SyntaxError {
                    range: 0..4,
                    want: Token::Date,
                    context: None,
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
                range: 8..9,
                want: Token::Sequence(Sequence::NumberOf(2, Character::Digit)),
                context: Some(Box::new(SyntaxError {
                    range: 0..9,
                    want: Token::Date,
                    context: None,
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
                range: 5..7,
                want: Token::Sequence(Sequence::NumberOf(2, Character::Digit)),
                context: Some(Box::new(SyntaxError {
                    range: 0..7,
                    want: Token::Date,
                    context: None,
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
                want: Token::Sequence(Sequence::One(Character::Digit)),
                context: Some(Box::new(SyntaxError {
                    range: 0..1,
                    want: Token::Decimal,
                    context: None,
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

    mod grouped_transaction {
        use super::*;
        use pretty_assertions::assert_eq;

        /// The bookings of the only transaction of `text`, as
        /// `(credit, debit, quantity, commodity)`.
        fn bookings(text: &str) -> Vec<(&str, &str, &str, &str)> {
            let tree = Parser::new(text).parse().expect("parses");
            let [Directive::Transaction(t)] = &tree.directives[..] else {
                panic!("want a single transaction, got {:?}", tree.directives);
            };
            t.bookings
                .iter()
                .map(|b| {
                    (
                        &text[b.credit.range.clone()],
                        &text[b.debit.range.clone()],
                        &text[b.quantity.0.clone()],
                        &text[b.commodity.0.clone()],
                    )
                })
                .collect()
        }

        #[test]
        fn parse_group() {
            let f = "2024-12-31\n  Message\nAssets:Foo\n-> Assets:Bar 4.23 USD\n";
            assert_eq!(
                Ok(Directive::Transaction(Transaction {
                    range: 0..55,
                    addon: None,
                    date: Date(0..10),
                    description: Description::Indented(vec![Range { start: 13, end: 20 }]),
                    bookings: Bookings::Groups(vec![Group {
                        range: 21..55,
                        accounts: vec![Leg {
                            range: 21..31,
                            account: Account {
                                range: 21..31,
                                segments: vec![21..27, 28..31]
                            },
                            amount: None,
                        }],
                        arrows: vec![Arrow {
                            direction: Direction::Out,
                            leg: Leg {
                                range: 35..54,
                                account: Account {
                                    range: 35..45,
                                    segments: vec![35..41, 42..45]
                                },
                                amount: Some(Amount {
                                    quantity: Decimal(46..50),
                                    commodity: Commodity(51..54),
                                }),
                            },
                        }],
                    }])
                })),
                Parser::new(f).parse_directive()
            );
        }

        /// One account fans out to an account per amount, and the groups of a
        /// transaction are read one after the other. The trailing whitespace
        /// of the example is deliberate.
        #[test]
        fn one_credit_to_many_debits() {
            let f = "@performance(VT,USD)\n\
                     2026-06-24 \n\
                     \x20 Buy 11 VT @ 154.45 USD\n\
                     Assets:Investments:IBKR       \n\
                     -> Expenses:Investments:Trading     1698.95 USD\n\
                     -> Expenses:Investments:Fees           1.00 USD\n\
                     Expenses:Investments:Trading\n\
                     -> Assets:Investments:IBKR               11 VT\n";
            assert_eq!(
                vec![
                    (
                        "Assets:Investments:IBKR",
                        "Expenses:Investments:Trading",
                        "1698.95",
                        "USD"
                    ),
                    (
                        "Assets:Investments:IBKR",
                        "Expenses:Investments:Fees",
                        "1.00",
                        "USD"
                    ),
                    (
                        "Expenses:Investments:Trading",
                        "Assets:Investments:IBKR",
                        "11",
                        "VT"
                    ),
                ],
                bookings(f)
            );
        }

        #[test]
        fn many_credits_to_one_debit() {
            let f =
                "2024-12-31\n  Message\nAssets:Foo 10 CHF\nAssets:Baz -2.5 CHF\n-> Assets:Bar\n";
            assert_eq!(
                vec![
                    ("Assets:Foo", "Assets:Bar", "10", "CHF"),
                    ("Assets:Baz", "Assets:Bar", "-2.5", "CHF"),
                ],
                bookings(f)
            );
        }

        /// `<-` reverses its own booking, so a group can collect what flows
        /// out of an account and what flows into it at once.
        #[test]
        fn arrows_point_both_ways() {
            let f = "2024-12-31\n  Message\n\
                     Assets:A\n\
                     -> Assets:B 5 CHF\n\
                     <- Assets:C 10 CHF\n\
                     <- Assets:B 4 VT\n";
            assert_eq!(
                vec![
                    ("Assets:A", "Assets:B", "5", "CHF"),
                    ("Assets:C", "Assets:A", "10", "CHF"),
                    ("Assets:B", "Assets:A", "4", "VT"),
                ],
                bookings(f)
            );
        }

        /// A single `<-` gives its direction to every account facing it.
        #[test]
        fn one_credit_behind_the_arrow() {
            let f = "2024-12-31\n  Message\n\
                     Assets:A 5 CHF\n\
                     Assets:B 4 CHF\n\
                     <- Assets:C\n";
            assert_eq!(
                vec![
                    ("Assets:C", "Assets:A", "5", "CHF"),
                    ("Assets:C", "Assets:B", "4", "CHF"),
                ],
                bookings(f)
            );
        }

        /// Reversing the arrow of a group of one booking is the same as
        /// swapping its accounts.
        #[test]
        fn reversing_the_arrow_swaps_the_accounts() {
            let out = "2024-12-31\n  Message\nAssets:A\n-> Assets:B 5 CHF\n";
            let into = "2024-12-31\n  Message\nAssets:A\n<- Assets:B 5 CHF\n";
            assert_eq!(vec![("Assets:A", "Assets:B", "5", "CHF")], bookings(out));
            assert_eq!(vec![("Assets:B", "Assets:A", "5", "CHF")], bookings(into));
        }

        /// The text of the description of the only transaction of `text`.
        fn description(text: &str) -> String {
            let tree = Parser::new(text).parse().expect("parses");
            let [Directive::Transaction(t)] = &tree.directives[..] else {
                panic!("want a single transaction, got {:?}", tree.directives);
            };
            t.description.text(text).into_owned()
        }

        /// The indentation of a line, and the whitespace at its end, are not
        /// part of the description.
        #[test]
        fn description_is_trimmed() {
            let f = "2024-12-31\n     Some message\t \nAssets:Foo\n-> Assets:Bar 1 CHF\n";
            assert_eq!("Some message", description(f));
        }

        /// Several indented lines are one description, and the line breaks
        /// between them are kept.
        #[test]
        fn description_spans_lines() {
            let f = "2024-12-31\n\
                     \x20 Buy 11 VT\n\
                     \tat 154.45 USD\n\
                     \x20     for the pension pot\n\
                     Assets:Foo\n\
                     -> Assets:Bar 1 CHF\n";
            assert_eq!(
                "Buy 11 VT\nat 154.45 USD\nfor the pension pot",
                description(f)
            );
        }

        /// The productions a failure names, the failure itself first.
        fn chain(text: &str) -> Vec<Token> {
            let mut tokens = Vec::new();
            let mut error = Parser::new(text).parse().err().map(Box::new);
            while let Some(e) = error {
                tokens.push(e.want);
                error = e.context;
            }
            tokens
        }

        /// A blank line ends the transaction, wherever it falls, so it never
        /// becomes an empty line of the description: what follows it has to
        /// be a group, and is not.
        #[test]
        fn description_stops_at_a_blank_line() {
            let f = "2024-12-31\n  Message\n   \nAssets:Foo\n-> Assets:Bar 1 CHF\n";
            let chain = chain(f);
            assert!(chain.contains(&Token::Group), "{chain:?}");
        }

        #[test]
        fn requires_a_description() {
            let f = "2024-12-31\nAssets:Foo\n-> Assets:Bar 1 CHF\n";
            assert_eq!(
                Some(Token::Description),
                Parser::new(f).parse().err().map(|e| e.want)
            );
        }

        /// The amounts must sit on exactly one side of the group, so that it
        /// is unambiguous which side the bookings fan out to.
        #[test]
        fn rejects_amounts_on_both_sides() {
            for f in [
                "2024-12-31\n  Message\nAssets:Foo 10 CHF\n-> Assets:Bar 10 CHF\n",
                "2024-12-31\n  Message\nAssets:Foo\n-> Assets:Bar\n",
                "2024-12-31\n  Message\nAssets:Foo\nAssets:Baz\n-> Assets:Bar 10 CHF\n",
                "2024-12-31\n  Message\nAssets:Foo 10 CHF\n-> Assets:Bar\n-> Assets:Qux\n",
                "2024-12-31\n  Message\nAssets:Foo 10 CHF\n",
                "2024-12-31\n  Message\nAssets:Foo\n",
                "2024-12-31\n  Message\nAssets:Foo 10 CHF\n<- Assets:Bar 10 CHF\n",
                "2024-12-31\n  Message\nAssets:Foo\n<- Assets:Bar\n",
            ] {
                assert_eq!(
                    Some(Token::Custom(GROUP_SHAPE.to_string())),
                    Parser::new(f).parse().err().map(|e| e.want),
                    "{f}"
                );
            }
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
                    description: Description::Quoted(QuotedString {
                        range: 11..20,
                        content: 12..19,
                    }),
                    bookings: Bookings::Lines(vec![Booking {
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
                    }])
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
