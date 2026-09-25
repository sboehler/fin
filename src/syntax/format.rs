use std::io::{self, Result, Write};

use crate::syntax::cst::VirtualAccount;

use super::cst::{
    Addon, Assertion, Bookings, Close, Directive, Group, Include, Leg, Open, Price, SubAssertion,
    SyntaxTree, Transaction,
};

pub fn format_file(w: &mut impl Write, source: &str, tree: &SyntaxTree) -> io::Result<()> {
    let n = initialize(tree, source);
    let mut pos = 0;
    for d in &tree.directives {
        w.write_all(&source.as_bytes()[pos..d.range().start])?;
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
                bookings,
                ..
            }) => {
                if let Some(a) = addon {
                    format_addon(w, a, source)?;
                    writeln!(w)?;
                }
                match bookings {
                    Bookings::Lines(bookings) => {
                        writeln!(
                            w,
                            "{date} {description}",
                            date = &source[date.0.clone()],
                            description = &source[description.range.clone()]
                        )?;
                        for b in bookings {
                            writeln!(
                                w,
                                "{credit:<width$} {debit:<width$} {amount:>10} {commodity}",
                                credit = &source[b.credit.range.clone()],
                                width = n,
                                debit = &source[b.debit.range.clone()],
                                amount = &source[b.quantity.0.clone()],
                                commodity = &source[b.commodity.0.clone()],
                            )?;
                        }
                    }
                    Bookings::Groups(groups) => {
                        writeln!(w, "{date}", date = &source[date.0.clone()])?;
                        writeln!(
                            w,
                            "  {description}",
                            description = &source[description.content.clone()]
                        )?;
                        for g in groups {
                            format_group(w, g, source, n)?;
                        }
                    }
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
                date, assertions, ..
            }) => {
                match &assertions[..] {
                    [
                        SubAssertion {
                            account,
                            balance: amount,
                            commodity,
                            ..
                        },
                    ] => write!(
                        w,
                        "{date} balance {account} {amount} {commodity}",
                        date = &source[date.0.clone()],
                        account = &source[account.range.clone()],
                        amount = &source[amount.0.clone()],
                        commodity = &source[commodity.0.clone()]
                    )?,
                    _ => {
                        writeln!(w, "{date} balance", date = &source[date.0.clone()])?;
                        for a in assertions {
                            writeln!(
                                w,
                                "{account} {amount} {commodity}",
                                account = &source[a.account.range.clone()],
                                amount = &source[a.balance.0.clone()],
                                commodity = &source[a.commodity.0.clone()]
                            )?;
                        }
                    }
                };
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
        pos = d.range().end
    }
    w.write_all(&source.as_bytes()[pos..tree.range.end])?;
    Ok(())
}

fn initialize(tree: &SyntaxTree, source: &str) -> usize {
    tree.directives
        .iter()
        .filter_map(|d| match d {
            Directive::Transaction(Transaction { bookings, .. }) => Some(bookings),
            _ => None,
        })
        .flat_map(Bookings::iter)
        .flat_map(|b| [b.credit, b.debit])
        .map(|a| source[a.range.clone()].chars().count())
        .max()
        .unwrap_or_default()
}

/// The credit accounts of a group, then its debit accounts marked with `->`.
/// The amounts of both shapes of group end up in the same column, which is
/// why the arrow counts towards the width of the account it precedes.
fn format_group(w: &mut impl Write, g: &Group, source: &str, width: usize) -> Result<()> {
    for leg in &g.credits {
        format_leg(w, leg, source, "", width + ARROW.len())?;
    }
    for leg in &g.debits {
        format_leg(w, leg, source, ARROW, width)?;
    }
    Ok(())
}

const ARROW: &str = "-> ";

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
            "{prefix}{account:<width$} {amount:>10} {commodity}",
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

    /// The accounts of both notations are aligned to the same width, and the
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

2026-06-25 \"Fee\"
Assets:Investments:IBKR Expenses:Investments:Fees 1 USD
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

2026-06-25 \"Fee\"
Assets:Investments:IBKR      Expenses:Investments:Fees             1 USD
",
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
Assets:Investments:IBKR 1698.95 USD
Expenses:Investments:Fees -1.00 USD
-> Expenses:Investments:Trading
";
        let once = format(source);
        assert_eq!(once, format(&once));
    }
}
