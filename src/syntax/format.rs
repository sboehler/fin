use std::io::{self, Result, Write};

use super::cst::{Addon, Assertion, Directive, SyntaxTree};

pub fn format_file(w: &mut impl Write, syntax_tree: &SyntaxTree) -> io::Result<()> {
    let n = initialize(syntax_tree);
    let mut pos = 0;
    for d in &syntax_tree.directives {
        w.write_all(syntax_tree.range.file.text[pos..d.range().start].as_bytes())?;
        match d {
            Directive::Include { path, .. } => {
                write!(w, "include {}", path.range.text())?;
            }
            Directive::Price {
                date,
                commodity,
                price,
                target,
                ..
            } => {
                write!(
                    w,
                    "{date} price {commodity} {price} {target}",
                    date = date.0.text(),
                    commodity = commodity.0.text(),
                    price = price.0.text(),
                    target = target.0.text(),
                )?;
            }
            Directive::Open { date, account, .. } => {
                write!(
                    w,
                    "{date} open {account}",
                    date = date.0.text(),
                    account = account.range.text(),
                )?;
            }
            Directive::Transaction {
                date,
                addon,
                description,
                bookings,
                ..
            } => {
                if let Some(a) = addon {
                    format_addon(w, a)?;
                    writeln!(w)?;
                }
                writeln!(
                    w,
                    "{date} {description}",
                    date = date.0.text(),
                    description = description.range.text()
                )?;
                for b in bookings {
                    writeln!(
                        w,
                        "{credit:<width$} {debit:<width$} {amount:>10} {commodity}",
                        credit = b.credit.range.text(),
                        width = n,
                        debit = b.debit.range.text(),
                        amount = b.quantity.0.text(),
                        commodity = b.commodity.0.text(),
                    )?;
                }
            }
            Directive::Assertion {
                date, assertions, ..
            } => {
                match &assertions[..] {
                    [Assertion {
                        account,
                        balance: amount,
                        commodity,
                        ..
                    }] => write!(
                        w,
                        "{date} balance {account} {amount} {commodity}",
                        date = date.0.text(),
                        account = account.range.text(),
                        amount = amount.0.text(),
                        commodity = commodity.0.text()
                    )?,
                    _ => {
                        writeln!(w, "{date} balance ", date = date.0.text())?;
                        for a in assertions {
                            writeln!(
                                w,
                                "{account} {amount} {commodity}",
                                account = a.account.range.text(),
                                amount = a.balance.0.text(),
                                commodity = a.commodity.0.text()
                            )?;
                        }
                    }
                };
            }
            Directive::Close { date, account, .. } => {
                write!(
                    w,
                    "{date} close {account}",
                    date = date.0.text(),
                    account = account.range.text(),
                )?;
            }
        }
        pos = d.range().end
    }
    w.write_all(syntax_tree.range.file.text[pos..syntax_tree.range.end].as_bytes())?;
    Ok(())
}

fn initialize(syntax_tree: &SyntaxTree) -> usize {
    syntax_tree
        .directives
        .iter()
        .filter_map(|d| match d {
            Directive::Transaction { bookings, .. } => Some(bookings),
            _ => None,
        })
        .flatten()
        .flat_map(|b| vec![&b.credit, &b.debit])
        .map(|a| a.range.text().chars().count())
        .max()
        .unwrap_or_default()
}

fn format_addon(w: &mut impl Write, a: &Addon) -> Result<()> {
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
            interval = interval.text(),
            start = start.0.text(),
            end = end.0.text(),
            account = account.range.text()
        ),
        Addon::Performance { commodities, .. } => {
            write!(w, "@performance(")?;
            for (i, c) in commodities.iter().enumerate() {
                w.write_all(c.0.text().as_bytes())?;
                if i < commodities.len() - 1 {
                    write!(w, ",")?;
                }
            }
            write!(w, ")")
        }
    }
}
