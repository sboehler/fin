use std::io::{self, Result, Write};

use super::cst::{
    Addon, Assertion, Close, Directive, Include, Open, Price, SubAssertion, SyntaxTree, Transaction,
};

pub fn format_file(w: &mut impl Write, source: &str, tree: &SyntaxTree) -> io::Result<()> {
    let n = initialize(tree, source);
    let mut pos = 0;
    for d in &tree.directives {
        w.write_all(source[pos..d.range().start].as_bytes())?;
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
            Directive::Assertion(Assertion {
                date, assertions, ..
            }) => {
                match &assertions[..] {
                    [SubAssertion {
                        account,
                        balance: amount,
                        commodity,
                        ..
                    }] => write!(
                        w,
                        "{date} balance {account} {amount} {commodity}",
                        date = &source[date.0.clone()],
                        account = &source[account.range.clone()],
                        amount = &source[amount.0.clone()],
                        commodity = &source[commodity.0.clone()]
                    )?,
                    _ => {
                        writeln!(w, "{date} balance ", date = &source[date.0.clone()])?;
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
    w.write_all(source[pos..tree.range.end].as_bytes())?;
    Ok(())
}

fn initialize(tree: &SyntaxTree, source: &str) -> usize {
    tree.directives
        .iter()
        .filter_map(|d| match d {
            Directive::Transaction(Transaction { bookings, .. }) => Some(bookings),
            _ => None,
        })
        .flatten()
        .flat_map(|b| vec![&b.credit, &b.debit])
        .map(|a| source[a.range.clone()].chars().count())
        .max()
        .unwrap_or_default()
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
