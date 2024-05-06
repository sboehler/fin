use std::io::{self, Result, Write};

use super::syntax::{Addon, Assertion, Command, Date, Directive, QuotedString, SyntaxTree};

pub fn format_file(w: &mut impl Write, s: &str, syntax_tree: &SyntaxTree) -> io::Result<()> {
    let n = initialize(s, &syntax_tree.directives);
    let mut pos = 0;
    for d in &syntax_tree.directives {
        match d {
            Directive::Include { range, path } => {
                w.write(s[pos..range.start].as_bytes())?;
                format_include(w, s, path)?;
                pos = range.end;
            }
            Directive::Dated {
                range,
                addon,
                date,
                command,
            } => {
                w.write(s[pos..range.start].as_bytes())?;
                format_dated(w, s, n, addon, date, command)?;
                pos = range.end;
            }
        }
    }
    w.write(s[pos..syntax_tree.range.end].as_bytes())?;
    Ok(())
}

fn initialize(text: &str, directives: &Vec<Directive>) -> usize {
    directives
        .iter()
        .filter_map(|d| match d {
            Directive::Dated {
                command: Command::Transaction { bookings, .. },
                ..
            } => Some(bookings),
            _ => None,
        })
        .flatten()
        .flat_map(|b| vec![&b.credit, &b.debit])
        .map(|a| a.range.slice(text).chars().count())
        .max()
        .unwrap_or_default()
}

fn format_include(w: &mut impl Write, text: &str, path: &QuotedString) -> Result<()> {
    write!(w, "include {}", path.range.slice(text))
}

fn format_dated(
    w: &mut impl Write,
    text: &str,
    n: usize,
    addon: &Option<Addon>,
    date: &Date,
    command: &Command,
) -> Result<()> {
    if let Some(a) = addon {
        format_addon(w, text, a)?;
        writeln!(w)?;
    }
    match command {
        Command::Price {
            commodity,
            price,
            target,
            ..
        } => write!(
            w,
            "{date} price {commodity} {price} {target}",
            date = date.0.slice(text),
            commodity = commodity.0.slice(text),
            price = price.0.slice(text),
            target = target.0.slice(text),
        ),
        Command::Open { account, .. } => write!(
            w,
            "{date} open {account}",
            date = date.0.slice(text),
            account = account.range.slice(text),
        ),
        Command::Transaction {
            description,
            bookings,
            ..
        } => {
            writeln!(
                w,
                "{date} {description}",
                date = date.0.slice(text),
                description = description.range.slice(text)
            )?;
            for b in bookings {
                writeln!(
                    w,
                    "{credit:<width$} {debit:<width$} {amount:>10} {commodity}",
                    credit = b.credit.range.slice(text),
                    width = n,
                    debit = b.debit.range.slice(text),
                    amount = b.quantity.0.slice(text),
                    commodity = b.commodity.0.slice(text),
                )?;
            }
            Ok(())
        }
        Command::Assertion { assertions, .. } => match &assertions[..] {
            [Assertion {
                account,
                balance: amount,
                commodity,
                ..
            }] => write!(
                w,
                "{date} balance {account} {amount} {commodity}",
                date = date.0.slice(text),
                account = account.range.slice(text),
                amount = amount.0.slice(text),
                commodity = commodity.0.slice(text)
            ),
            _ => {
                writeln!(w, "{date} balance ", date = date.0.slice(text))?;
                for a in assertions {
                    writeln!(
                        w,
                        "{account} {amount} {commodity}",
                        account = a.account.range.slice(text),
                        amount = a.balance.0.slice(text),
                        commodity = a.commodity.0.slice(text)
                    )?;
                }
                Ok(())
            }
        },
        Command::Close { account, .. } => write!(
            w,
            "{date} close {account}",
            date = date.0.slice(text),
            account = account.range.slice(text),
        ),
    }
}

fn format_addon(w: &mut impl Write, text: &str, a: &Addon) -> Result<()> {
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
            interval = interval.slice(text),
            start = start.0.slice(text),
            end = end.0.slice(text),
            account = account.range.slice(text)
        ),
        Addon::Performance { commodities, .. } => {
            write!(w, "@performance(")?;
            for (i, c) in commodities.iter().enumerate() {
                w.write(c.0.slice(text).as_bytes())?;
                if i < commodities.len() - 1 {
                    write!(w, ",")?;
                }
            }
            write!(w, ")")
        }
    }
}
