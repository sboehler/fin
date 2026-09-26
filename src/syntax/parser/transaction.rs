use std::ops::Range;

use crate::syntax::cst::{
    Addon, Amount, Arrow, Booking, Bookings, Character, Date, Description, Direction, Directive,
    Group, Leg, Token, Transaction,
};
use crate::syntax::scanner::Scanner;
use crate::syntax::scope::Scope;

use super::Result;
use super::lexical::{account, commodity, decimal, quoted_string};

pub(super) fn transaction(scope: &Scope, addon: Option<Addon>, date: Date) -> Result<Directive> {
    let s = scope.scanner();
    let scope = scope.with(Token::Transaction);
    let description = Description::Quoted(quoted_string(s)?);
    s.read_rest_of_line()?;
    let mut bookings = Vec::new();
    loop {
        bookings.push(booking(s)?);
        s.read_rest_of_line()?;
        if !s.current().is_some_and(char::is_alphanumeric) {
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
pub(super) fn grouped_transaction(
    scope: &Scope,
    addon: Option<Addon>,
    date: Date,
) -> Result<Directive> {
    let s = scope.scanner();
    let scope = scope.with(Token::Transaction);
    s.read_rest_of_line()?;
    let description = indented_description(s)?;
    let mut groups = Vec::new();
    loop {
        groups.push(group(s)?);
        if !Character::Alphabetic.is(s.current()) {
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
fn indented_description(s: &Scanner) -> Result<Description> {
    let scope = s.enter(Token::Description);
    let mut lines = Vec::new();
    loop {
        let rollback = s.snapshot();
        if !Character::HorizontalSpace.is(s.current()) {
            break;
        }
        s.read_space();
        let line = trim_end(scope.source(), s.read_until(&Character::NewLine));
        if line.is_empty() {
            rollback();
            break;
        }
        s.read_rest_of_line()?;
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
fn group(s: &Scanner) -> Result<Group> {
    let scope = s.enter(Token::Group);
    let mut accounts = Vec::new();
    loop {
        accounts.push(leg(s)?);
        s.read_rest_of_line()?;
        if !Character::Alphabetic.is(s.current()) {
            break;
        }
    }
    let mut arrows = Vec::new();
    loop {
        let direction = match s.current() {
            Some('-') => Direction::Out,
            Some('<') => Direction::In,
            _ => break,
        };
        s.read_string(match direction {
            Direction::Out => "->",
            Direction::In => "<-",
        })?;
        s.read_space_1()?;
        let leg = leg(s)?;
        s.read_rest_of_line()?;
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
fn leg(s: &Scanner) -> Result<Leg> {
    let scope = s.enter(Token::Booking);
    let account = account(s)?;
    let mut range = scope.range();
    s.read_space();
    let amount = match s.current() {
        Some(c) if c.is_ascii_digit() || c == '-' => {
            let quantity = decimal(s, Token::Quantity)?;
            s.read_space_1()?;
            let commodity = commodity(s)?;
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

fn booking(s: &Scanner) -> Result<Booking> {
    let scope = s.enter(Token::Booking);
    let credit = account(s)?;
    s.read_space_1()?;
    let debit = account(s)?;
    s.read_space_1()?;
    let quantity = decimal(s, Token::Quantity)?;
    s.read_space_1()?;
    let commodity = commodity(s)?;
    Ok(Booking {
        range: scope.range(),
        credit,
        debit,
        quantity,
        commodity,
    })
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
    use super::super::directive::directive;
    use super::super::parse;
    use super::*;
    use crate::syntax::cst::{Account, Commodity, Date, Decimal, QuotedString};
    use crate::syntax::scanner::Scanner;
    use pretty_assertions::assert_eq;

    mod grouped_transaction {
        use super::*;
        use pretty_assertions::assert_eq;

        /// The bookings of the only transaction of `text`, as
        /// `(credit, debit, quantity, commodity)`.
        fn bookings(text: &str) -> Vec<(&str, &str, &str, &str)> {
            let tree = parse(text).expect("parses");
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
                directive(&Scanner::new(f))
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
            let tree = parse(text).expect("parses");
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
            let mut error = parse(text).err().map(Box::new);
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
            assert_eq!(Some(Token::Description), parse(f).err().map(|e| e.want));
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
                    parse(f).err().map(|e| e.want),
                    "{f}"
                );
            }
        }
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
            directive(&Scanner::new(f))
        );
    }
}
