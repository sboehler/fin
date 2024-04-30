use std::{
    cmp::max,
    io::{self, Result, Write},
};

use super::syntax::{
    Addon, Assertion, Command, Date, Directive, QuotedString, SourceFile,
};

pub fn format_file(
    w: &mut impl Write,
    source_file: &SourceFile,
) -> io::Result<()> {
    let n = initialize(&source_file.directives);
    let text = &source_file.range.str.as_bytes();
    let mut pos = 0;
    for d in &source_file.directives {
        match d {
            Directive::Include {
                range,
                path,
            } => {
                w.write(&text[pos..range.start])?;
                format_include(w, path)?;
                pos = range.start + range.str.len();
            }
            Directive::Dated {
                range,
                addons,
                date,
                command,
            } => {
                w.write(&text[pos..range.start])?;
                format_dated(w, n, addons, date, command)?;
                pos = range.start + range.str.len();
            }
        }
    }
    w.write(&text[pos..source_file.range.str.len()])?;
    Ok(())
}

fn initialize(directives: &Vec<Directive>) -> usize {
    directives
        .iter()
        .flat_map(|d| match d {
            Directive::Dated {
                command:
                    Command::Transaction {
                        bookings,
                        ..
                    },
                ..
            } => Some(bookings),
            _ => None,
        })
        .flatten()
        .map(|b| {
            max(
                b.credit.range.str.chars().count(),
                b.debit.range.str.chars().count(),
            )
        })
        .max()
        .unwrap_or_default()
}

fn format_include(w: &mut impl Write, path: &QuotedString) -> Result<()> {
    write!(w, "include {}", path.range.str)
}

fn format_dated(
    w: &mut impl Write,
    n: usize,
    addons: &Vec<Addon>,
    date: &Date,
    command: &Command,
) -> Result<()> {
    for a in addons {
        format_addon(w, a)?;
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
            date = date.0.str,
            commodity = commodity.0.str,
            price = price.0.str,
            target = target.0.str,
        ),
        Command::Open {
            account,
            ..
        } => write!(
            w,
            "{date} open {account}",
            date = date.0.str,
            account = account.range.str,
        ),
        Command::Transaction {
            description,
            bookings,
            ..
        } => {
            writeln!(
                w,
                "{date} {description}",
                date = date.0.str,
                description = description.range.str
            )?;
            for b in bookings {
                writeln!(
                    w,
                    "{credit:<width$} {debit:<width$} {amount:>10} {commodity}",
                    credit = b.credit.range.str,
                    width = n,
                    debit = b.debit.range.str,
                    amount = b.quantity.0.str,
                    commodity = b.commodity.0.str,
                )?;
            }
            Ok(())
        }
        Command::Assertion {
            assertions,
            ..
        } => match &assertions[..] {
            [Assertion {
                account,
                amount,
                commodity,
                ..
            }] => write!(
                w,
                "{date} balance {account} {amount} {commodity}",
                date = date.0.str,
                account = account.range.str,
                amount = amount.0.str,
                commodity = commodity.0.str
            ),
            _ => {
                writeln!(w, "{date} balance ", date = date.0.str)?;
                for a in assertions {
                    writeln!(
                        w,
                        "{account} {amount} {commodity}",
                        account = a.account.range.str,
                        amount = a.amount.0.str,
                        commodity = a.commodity.0.str
                    )?;
                }
                Ok(())
            }
        },
        Command::Close {
            account,
            ..
        } => write!(
            w,
            "{date} close {account}",
            date = date.0.str,
            account = account.range.str,
        ),
    }
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
            "@accrue {} {} {} {}",
            interval.str, start.0.str, end.0.str, account.range.str
        ),
        Addon::Performance {
            commodities,
            ..
        } => {
            write!(w, "@performance(")?;
            for (i, c) in commodities.iter().enumerate() {
                w.write(c.0.str.as_bytes())?;
                if i < commodities.len() - 1 {
                    write!(w, ",")?;
                }
            }
            write!(w, ")")
        }
    }
}
