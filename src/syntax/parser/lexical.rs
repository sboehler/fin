use std::ops::Range;

use crate::syntax::cst::{
    Account, Character, Commodity, Date, Decimal, QuotedString, Sequence, Token,
};
use crate::syntax::scanner::Scanner;

use super::Result;

pub(super) fn account(s: &Scanner) -> Result<Account> {
    let scope = s.enter(Token::Account);
    let account_type = account_type(s)?;
    let mut segments = vec![account_type];
    while s.current() == Some(':') {
        s.read_char(&Character::Char(':'))?;
        segments.push(s.read_while_1(&Character::AlphaNum)?);
    }
    Ok(Account {
        range: scope.range(),
        segments,
    })
}

fn account_type(s: &Scanner) -> Result<Range<usize>> {
    let scope = s.enter(Token::AccountType);
    s.read_while_1(&Character::Alphabetic)
        .and_then(|r| match &scope.source()[r.clone()] {
            "Assets" | "Liabilities" | "Expenses" | "Equity" | "Income" => Ok(r.clone()),
            _ => Err(scope.token_error()),
        })
}

pub(super) fn commodity(s: &Scanner) -> Result<Commodity> {
    let _scope = s.enter(Token::Commodity);
    s.read_while_1(&Character::AlphaNum).map(Commodity)
}

pub(super) fn date(s: &Scanner) -> Result<Date> {
    let scope = s.enter(Token::Date);
    s.read_sequence(&Sequence::NumberOf(4, Character::Digit))?;
    s.read_char(&Character::Char('-'))?;
    s.read_sequence(&Sequence::NumberOf(2, Character::Digit))?;
    s.read_char(&Character::Char('-'))?;
    s.read_sequence(&Sequence::NumberOf(2, Character::Digit))?;
    Ok(Date(scope.range()))
}

pub(super) fn interval(s: &Scanner) -> Result<Range<usize>> {
    let scope = s.enter(Token::Interval);
    match s.current() {
        Some('d') => s.read_string("daily"),
        Some('w') => s.read_string("weekly"),
        Some('m') => s.read_string("monthly"),
        Some('q') => s.read_string("quarterly"),
        Some('y') => s.read_string("yearly"),
        Some('o') => s.read_string("once"),
        _o => Err(scope.token_error()),
    }
}

pub(super) fn decimal(s: &Scanner, token: Token) -> Result<Decimal> {
    let scope = s.enter(token);
    if let Some('-') = s.current() {
        s.read_char(&Character::Char('-'))?;
    }
    s.read_while_1(&Character::Digit)?;
    if let Some('.') = s.current() {
        s.read_char(&Character::Char('.'))?;
        s.read_while_1(&Character::Digit)?;
    }
    Ok(Decimal(scope.range()))
}

pub(super) fn quoted_string(s: &Scanner) -> Result<QuotedString> {
    let scope = s.enter(Token::QuotedString);
    s.read_char(&Character::Char('"'))?;
    let content = s.read_while(&Character::NotChar('"'));
    s.read_char(&Character::Char('"'))?;
    Ok(QuotedString {
        range: scope.range(),
        content,
    })
}

pub(super) fn comment(s: &Scanner) -> Result<Range<usize>> {
    let scope = s.enter(Token::Comment);
    match s.current() {
        Some('#') | Some('*') => {
            s.read_until(&Character::NewLine);
            let range = scope.range();
            s.read_char(&Character::NewLine)?;
            Ok(range)
        }
        Some('/') => {
            s.read_string("//")?;
            s.read_until(&Character::NewLine);
            let range = scope.range();
            s.read_char(&Character::NewLine)?;
            Ok(range)
        }
        _o => Err(scope.token_error()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::cst::Sequence;
    use crate::syntax::error::SyntaxError;
    use crate::syntax::scanner::Scanner;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_parse_commodity1() {
        let text = "USD";
        assert_eq!(Ok(Commodity(0..3)), commodity(&Scanner::new(text)));
    }

    #[test]
    fn test_parse_commodity2() {
        assert_eq!(Ok(Commodity(0..4)), commodity(&Scanner::new("1FOO")));
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
            commodity(&Scanner::new(text))
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
            commodity(&Scanner::new("/USD"))
        );
    }

    #[test]
    fn test_parse_account() {
        assert_eq!(
            Ok(Account {
                range: 0..6,
                segments: vec![Range { start: 0, end: 6 }],
            }),
            account(&Scanner::new("Assets")),
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
            account(&Scanner::new(f2)),
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
            account(&Scanner::new(f3)),
        );
    }

    #[test]
    fn test_parse_date1() {
        let f = "2024-05-07";
        assert_eq!(Ok(Date(0..10)), date(&Scanner::new(f)),);
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
            date(&Scanner::new(f)),
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
            date(&Scanner::new(f)),
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
            date(&Scanner::new(f))
        )
    }

    #[test]
    fn test_parse_interval() {
        for d in ["daily", "weekly", "monthly", "quarterly", "yearly", "once"] {
            assert_eq!(Ok(d), interval(&Scanner::new(d)).map(|r| &d[r]),);
        }
    }

    #[test]
    fn test_parse_decimal() {
        for d in ["0", "10.01", "-10.01"] {
            assert_eq!(
                Ok(Decimal(0..d.len())),
                decimal(&Scanner::new(d), Token::Decimal),
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
            decimal(&Scanner::new(f), Token::Decimal),
        );
    }
}
