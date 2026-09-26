use std::io::{self, Result, Write};
use std::ops::Range;

use crate::syntax::cst::VirtualAccount;

use super::cst::{
    Addon, Assertion, Close, Date, Direction, Directive, Group, Include, Leg, Open, Price,
    SyntaxTree, Transaction,
};

pub fn format_file(w: &mut impl Write, source: &str, tree: &SyntaxTree) -> io::Result<()> {
    let n = initialize(tree, source);
    let mut pos = 0;
    let mut directives = tree.directives.iter().peekable();
    while let Some(d) = directives.next() {
        w.write_all(&source.as_bytes()[pos..d.range().start])?;
        pos = d.range().end;
        match d {
            Directive::Include(Include { path, .. }) => {
                write!(w, "include {}", &source[path.range.clone()])?;
            }
            Directive::Price(Price {
                date,
                commodity,
                price,
                target,
                ..
            }) => {
                write!(
                    w,
                    "{date} price {commodity} {price} {target}",
                    date = &source[date.0.clone()],
                    commodity = &source[commodity.0.clone()],
                    price = &source[price.0.clone()],
                    target = &source[target.0.clone()],
                )?;
            }
            Directive::Open(Open { date, account, .. }) => {
                write!(
                    w,
                    "{date} open {account}",
                    date = &source[date.0.clone()],
                    account = &source[account.range.clone()],
                )?;
            }
            Directive::Transaction(Transaction {
                date,
                addon,
                description,
                groups,
                ..
            }) => {
                if let Some(a) = addon {
                    format_addon(w, a, source)?;
                    writeln!(w)?;
                }
                writeln!(w, "{date}", date = &source[date.0.clone()])?;
                // Each line of the description keeps its own line, so that a
                // rewritten transaction reads as it was typed.
                for line in &description.0 {
                    writeln!(w, "  {line}", line = &source[line.clone()])?;
                }
                for g in groups {
                    format_group(w, g, source, n)?;
                }
            }
            Directive::VirtualAccount(VirtualAccount {
                account, patterns, ..
            }) => {
                writeln!(
                    w,
                    "virtual {account}",
                    account = &source[account.range.clone()]
                )?;
                for pattern in patterns {
                    writeln!(w, "{pattern}", pattern = &source[pattern.clone()])?
                }
            }
            Directive::Assertion(Assertion {
                range,
                date,
                assertions,
            }) => {
                let mut range = range;
                let mut assertions = assertions.iter().collect::<Vec<_>>();
                // The balance directives which follow this one on the same
                // date are written as part of it, so that what one statement
                // asserts reads as one directive however it was written.
                while let Some(a) = directives
                    .peek()
                    .copied()
                    .and_then(|next| merges_into(next, range, date, source))
                {
                    directives.next();
                    assertions.extend(&a.assertions);
                    range = &a.range;
                    pos = a.range.end;
                }
                // Every balance directive is written as a group: the accounts
                // below the date, at column zero, with their amounts in the
                // column the amounts of the transactions are in.
                writeln!(w, "{date} balance", date = &source[date.0.clone()])?;
                let lines = assertions
                    .iter()
                    .map(|a| {
                        format!(
                            "{account:<width$} {amount:>AMOUNT_WIDTH$} {commodity}",
                            account = &source[a.account.range.clone()],
                            width = n + ARROW_WIDTH,
                            amount = &source[a.balance.0.clone()],
                            commodity = &source[a.commodity.0.clone()]
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                w.write_all(lines.as_bytes())?;
                // The directive takes in the newline which ends its last line,
                // and so has to write it back.
                writeln!(w)?;
            }
            Directive::Close(Close { date, account, .. }) => {
                write!(
                    w,
                    "{date} close {account}",
                    date = &source[date.0.clone()],
                    account = &source[account.range.clone()],
                )?;
            }
        }
    }
    w.write_all(&source.as_bytes()[pos..tree.range.end])?;
    Ok(())
}

fn initialize(tree: &SyntaxTree, source: &str) -> usize {
    tree.directives
        .iter()
        .filter_map(|d| match d {
            Directive::Transaction(t) => Some(t),
            _ => None,
        })
        .flat_map(Transaction::bookings)
        .flat_map(|b| [b.credit, b.debit])
        .map(|a| source[a.range.clone()].chars().count())
        .max()
        .unwrap_or_default()
}

/// The directive if it is a balance directive on `date` which nothing but
/// whitespace separates from the directive ending at `range`, and which is
/// therefore written as part of it.
fn merges_into<'a>(
    d: &'a Directive,
    range: &Range<usize>,
    date: &Date,
    source: &str,
) -> Option<&'a Assertion> {
    match d {
        Directive::Assertion(a)
            if source[a.date.0.clone()] == source[date.0.clone()]
                && source[range.end..a.range.start].trim().is_empty() =>
        {
            Some(a)
        }
        _ => None,
    }
}

/// The accounts of a group at column zero, then the accounts facing them with
/// their arrows. The amounts of both shapes of group end up in the same
/// column, which is why the arrow counts towards the width of the account it
/// precedes.
fn format_group(w: &mut impl Write, g: &Group, source: &str, width: usize) -> Result<()> {
    for leg in &g.accounts {
        format_leg(w, leg, source, "", width + ARROW_WIDTH)?;
    }
    for arrow in &g.arrows {
        let prefix = match arrow.direction {
            Direction::Out => "-> ",
            Direction::In => "<- ",
        };
        format_leg(w, &arrow.leg, source, prefix, width)?;
    }
    Ok(())
}

/// The width of an arrow and the space after it. Both arrows are the same
/// width, so neither disturbs the alignment.
const ARROW_WIDTH: usize = 3;

/// The width of the column every amount is right-aligned in.
const AMOUNT_WIDTH: usize = 10;

fn format_leg(
    w: &mut impl Write,
    leg: &Leg,
    source: &str,
    prefix: &str,
    width: usize,
) -> Result<()> {
    let account = &source[leg.account.range.clone()];
    match &leg.amount {
        None => writeln!(w, "{prefix}{account}"),
        Some(a) => writeln!(
            w,
            "{prefix}{account:<width$} {amount:>AMOUNT_WIDTH$} {commodity}",
            amount = &source[a.quantity.0.clone()],
            commodity = &source[a.commodity.0.clone()],
        ),
    }
}

fn format_addon(w: &mut impl Write, a: &Addon, source: &str) -> Result<()> {
    match a {
        Addon::Accrual {
            interval,
            start,
            end,
            account,
            ..
        } => write!(
            w,
            "@accrue {interval} {start} {end} {account}",
            interval = &source[interval.clone()],
            start = &source[start.0.clone()],
            end = &source[end.0.clone()],
            account = &source[account.range.clone()]
        ),
        Addon::Performance { commodities, .. } => {
            write!(w, "@performance(")?;
            for (i, c) in commodities.iter().enumerate() {
                w.write_all(source[c.0.clone()].as_bytes())?;
                if i < commodities.len() - 1 {
                    write!(w, ",")?;
                }
            }
            write!(w, ")")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::parse_text;
    use pretty_assertions::assert_eq;

    fn format(source: &str) -> String {
        let tree = parse_text(source).expect("parses");
        let mut w = Vec::new();
        format_file(&mut w, source, &tree).unwrap();
        String::from_utf8(w).unwrap()
    }

    /// The accounts of every group are aligned to the same width, and the
    /// arrow counts towards it, so every amount in the file is in one column.
    #[test]
    fn formats_groups() {
        let source = "\
# comment
2026-06-24 \n\
\x20   Buy 11 VT
Assets:Investments:IBKR   \n\
->  Expenses:Investments:Trading  1698.95 USD
-> Expenses:Investments:Fees 1.00 USD
Expenses:Investments:Trading
->   Assets:Investments:IBKR 11 VT
";
        assert_eq!(
            "\
# comment
2026-06-24
  Buy 11 VT
Assets:Investments:IBKR
-> Expenses:Investments:Trading    1698.95 USD
-> Expenses:Investments:Fees          1.00 USD
Expenses:Investments:Trading
-> Assets:Investments:IBKR              11 VT
",
            format(source)
        );
    }

    /// The accounts of a balance directive are written below its date, with
    /// their amounts in the column the amounts of the transactions are in, and
    /// the directives on one date are written as one.
    #[test]
    fn formats_balances_as_groups() {
        let source = "\
2026-06-24
  Buy 11 VT
Assets:Investments:IBKR
-> Expenses:Investments:Trading 1698.95 USD

2026-06-25 balance
Assets:IBKR 100 USD

2026-06-25 balance
Assets:Investments:IBKR 11 VT

2026-06-26 balance
Assets:Investments:IBKR 1698.95 USD
Income:Dividends 5 USD
";
        assert_eq!(
            "\
2026-06-24
  Buy 11 VT
Assets:Investments:IBKR
-> Expenses:Investments:Trading    1698.95 USD

2026-06-25 balance
Assets:IBKR                            100 USD
Assets:Investments:IBKR                 11 VT

2026-06-26 balance
Assets:Investments:IBKR            1698.95 USD
Income:Dividends                         5 USD
",
            format(source)
        );
    }

    /// Every line of a description keeps its own line, indented by two
    /// spaces whatever it was indented by.
    #[test]
    fn formats_multiline_descriptions() {
        let source = "2026-06-24\n\
                      \x20     Buy 11 VT\n\
                      \tat 154.45 USD\n\
                      Assets:IBKR\n\
                      -> Expenses:Trading 1698.95 USD\n";
        assert_eq!(
            "2026-06-24\n\
             \x20 Buy 11 VT\n\
             \x20 at 154.45 USD\n\
             Assets:IBKR\n\
             -> Expenses:Trading    1698.95 USD\n",
            format(source)
        );
    }

    /// Both arrows are kept as they were written, and both are the same
    /// width, so the amounts stay in one column.
    #[test]
    fn formats_both_arrows() {
        let source = "2026-06-24\n\
                      \x20 Rebalance\n\
                      Assets:IBKR\n\
                      ->   Expenses:Trading 5 CHF\n\
                      <-  Income:Dividends 10 CHF\n";
        assert_eq!(
            "2026-06-24\n\
             \x20 Rebalance\n\
             Assets:IBKR\n\
             -> Expenses:Trading          5 CHF\n\
             <- Income:Dividends         10 CHF\n",
            format(source)
        );
    }

    /// Formatting is idempotent: the canonical form of a file formats to
    /// itself.
    #[test]
    fn is_idempotent() {
        let source = "\
@performance(VT)
2026-06-24
  Buy 11 VT
  at 154.45 USD
Assets:Investments:IBKR 1698.95 USD
Expenses:Investments:Fees -1.00 USD
-> Expenses:Investments:Trading

2026-06-25 balance
Assets:IBKR 100 USD

2026-06-26 balance
Assets:Investments:IBKR 1698.95 USD
";
        let once = format(source);
        assert_eq!(once, format(&once));
    }
}
