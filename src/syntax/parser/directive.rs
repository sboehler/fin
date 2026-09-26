use crate::syntax::cst::{
    Addon, Assertion, Character, Close, Date, Directive, Include, Open, Price, SubAssertion, Token,
    VirtualAccount,
};
use crate::syntax::scanner::Scanner;
use crate::syntax::scope::Scope;

use super::Result;
use super::lexical::{account, commodity, date, decimal, interval, quoted_string};
use super::transaction::{grouped_transaction, transaction};

pub(super) fn directive(s: &Scanner) -> Result<Directive> {
    let scope = s.enter(Token::Directive);
    match s.current() {
        Some('i') => include(&scope),
        Some('v') => virtual_account(&scope),
        Some(c) if c.is_ascii_digit() || c == '@' => command(&scope),
        _o => Err(scope.token_error()),
    }
}

fn command(scope: &Scope) -> Result<Directive> {
    let s = scope.scanner();
    let mut addon = None;
    if let Some('@') = s.current() {
        addon = Some(self::addon(s)?);
        s.read_rest_of_line()?;
    }
    let date = date(s)?;
    s.read_space_1()?;

    let command = match s.current() {
        Some('p') => price(scope, date)?,
        Some('o') => open(scope, date)?,
        Some('"') => transaction(scope, addon, date)?,
        Some('b') => assertion(scope, date)?,
        Some('c') => close(scope, date)?,
        // Nothing else on the date line: the description is on the line
        // below it, and the bookings are written as groups.
        Some('\n') => grouped_transaction(scope, addon, date)?,
        _o => Err(scope.token_error())?,
    };
    s.read_rest_of_line()?;
    Ok(command)
}

fn include(scope: &Scope) -> Result<Directive> {
    let s = scope.scanner();
    let scope = scope.with(Token::Include);
    s.read_string("include")?;
    s.read_space_1()?;
    let path = quoted_string(s)?;
    Ok(Directive::Include(Include {
        range: scope.range(),
        path,
    }))
}

fn virtual_account(scope: &Scope) -> Result<Directive> {
    let s = scope.scanner();
    let scope = scope.with(Token::VirtualAccount);
    s.read_string("virtual")?;
    s.read_space_1()?;
    let account = account(s)?;
    s.read_rest_of_line()?;
    let mut patterns = Vec::new();
    while s.current().is_some_and(char::is_alphanumeric) {
        let pattern = s.read_until(&Character::WhiteSpace);
        s.read_rest_of_line()?;
        patterns.push(pattern);
    }
    Ok(Directive::VirtualAccount(VirtualAccount {
        range: scope.range(),
        account,
        patterns,
    }))
}

fn addon(s: &Scanner) -> Result<Addon> {
    let scope = s.enter(Token::Addon);
    s.read_char(&Character::Char('@'))?;
    match s.current() {
        Some('p') => performance(&scope),
        Some('a') => accrual(&scope),
        _o => Err(scope.token_error())?,
    }
}

fn performance(scope: &Scope) -> Result<Addon> {
    let s = scope.scanner();
    let scope = scope.with(Token::Performance);
    s.read_string("performance")?;
    s.read_space();
    s.read_char(&Character::Char('('))?;
    s.read_space();
    let mut commodities = Vec::new();
    while s.current().is_some_and(char::is_alphanumeric) {
        commodities.push(commodity(s)?);
        s.read_space();
        if let Some(',') = s.current() {
            s.read_char(&Character::Char(','))?;
            s.read_space();
        }
    }
    s.read_char(&Character::Char(')'))?;
    Ok(Addon::Performance {
        range: scope.range(),
        commodities,
    })
}

fn accrual(scope: &Scope) -> Result<Addon> {
    let s = scope.scanner();
    let scope = scope.with(Token::Accrual);
    s.read_string("accrue")?;
    s.read_space_1()?;
    let interval = interval(s)?;
    s.read_space_1()?;
    let start_date = date(s)?;
    s.read_space_1()?;
    let end_date = date(s)?;
    s.read_space_1()?;
    let account = account(s)?;
    Ok(Addon::Accrual {
        range: scope.range(),
        interval,
        start: start_date,
        end: end_date,
        account,
    })
}

fn price(scope: &Scope, date: Date) -> Result<Directive> {
    let s = scope.scanner();
    let scope = scope.with(Token::Price);
    s.read_string("price")?;
    s.read_space_1()?;
    let commodity = self::commodity(s)?;
    s.read_space_1()?;
    let price = decimal(s, Token::Price)?;
    s.read_space_1()?;
    let target = self::commodity(s)?;
    Ok(Directive::Price(Price {
        range: scope.range(),
        date,
        commodity,
        price,
        target,
    }))
}

fn open(scope: &Scope, date: Date) -> Result<Directive> {
    let s = scope.scanner();
    let scope = scope.with(Token::Open);
    s.read_string("open")?;
    s.read_space_1()?;
    let a = account(s)?;
    Ok(Directive::Open(Open {
        range: scope.range(),
        date,
        account: a,
    }))
}

fn assertion(scope: &Scope, date: Date) -> Result<Directive> {
    let s = scope.scanner();
    let scope = scope.with(Token::Assertion);
    s.read_string("balance")?;
    s.read_space_1()?;
    let mut assertions = Vec::new();
    if let Some('\n') = s.current() {
        s.read_rest_of_line()?;
        loop {
            assertions.push(sub_assertion(s)?);
            s.read_rest_of_line()?;
            if !Character::AlphaNum.is(s.current()) {
                break;
            }
        }
    } else {
        assertions.push(sub_assertion(s)?);
    }
    Ok(Directive::Assertion(Assertion {
        range: scope.range(),
        date,
        assertions,
    }))
}

fn sub_assertion(s: &Scanner) -> Result<SubAssertion> {
    let scope = s.enter(Token::SubAssertion);
    let account = account(s)?;
    s.read_space_1()?;
    let amount = decimal(s, Token::Quantity)?;
    s.read_space_1()?;
    let commodity = commodity(s)?;
    Ok(SubAssertion {
        range: scope.range(),
        account,
        balance: amount,
        commodity,
    })
}

fn close(scope: &Scope, date: Date) -> Result<Directive> {
    let s = scope.scanner();
    let scope = scope.with(Token::Close);
    s.read_string("close")?;
    s.read_space_1()?;
    let account = account(s)?;
    Ok(Directive::Close(Close {
        range: scope.range(),
        date,
        account,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::cst::{Account, Commodity, Decimal, QuotedString};
    use crate::syntax::scanner::Scanner;
    use pretty_assertions::assert_eq;

    mod addon {
        use super::super::addon;
        use crate::syntax::cst::{Account, Addon, Commodity, Date};
        use crate::syntax::scanner::Scanner;
        use pretty_assertions::assert_eq;

        #[test]
        fn performance() {
            let f1 = "@performance( USD  , VT)";
            assert_eq!(
                Ok(Addon::Performance {
                    range: 0..24,
                    commodities: vec![Commodity(14..17), Commodity(21..23),]
                }),
                addon(&Scanner::new(f1))
            );
            let f2 = "@performance(  )";
            assert_eq!(
                Ok(Addon::Performance {
                    range: 0..16,
                    commodities: vec![]
                }),
                addon(&Scanner::new(f2)),
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
                addon(&Scanner::new(f))
            )
        }
    }

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
            directive(&Scanner::new(f))
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
            directive(&Scanner::new(f))
        )
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
            directive(&Scanner::new(f))
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
            directive(&Scanner::new(f))
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
            directive(&Scanner::new(f))
        )
    }
}
