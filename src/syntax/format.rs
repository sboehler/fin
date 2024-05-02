use std::{
    cmp::max,
    io::{self, Result, Write},
};

use super::syntax::{
    Addon, Assertion, Command, Date, Directive, QuotedString, SourceFile,
};

pub fn format_file(
    w: &mut impl Write,
    s: &str,
    source_file: &SourceFile,
) -> io::Result<()> {
    let n = initialize(s, &source_file.directives);
    let mut pos = 0;
    for d in &source_file.directives {
        match d {
            Directive::Include {
                range,
                path,
            } => {
                w.write(s[pos..range.start].as_bytes())?;
                format_include(w, s, path)?;
                pos = range.end;
            }
            Directive::Dated {
                range,
                addons,
                date,
                command,
            } => {
                w.write(s[pos..range.start].as_bytes())?;
                format_dated(w, s, n, addons, date, command)?;
                pos = range.end;
            }
        }
    }
    w.write(s[pos..source_file.range.end].as_bytes())?;
    Ok(())
}

fn initialize(text: &str, directives: &Vec<Directive>) -> usize {
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
                text[b.credit.range.range()].chars().count(),
                text[b.debit.range.range()].chars().count(),
            )
        })
        .max()
        .unwrap_or_default()
}

fn format_include(
    w: &mut impl Write,
    text: &str,
    path: &QuotedString,
) -> Result<()> {
    write!(w, "include {}", &text[path.range.range()])
}

fn format_dated(
    w: &mut impl Write,
    text: &str,
    n: usize,
    addons: &Vec<Addon>,
    date: &Date,
    command: &Command,
) -> Result<()> {
    for a in addons {
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
            date = &text[date.0.range()],
            commodity = &text[commodity.0.range()],
            price = &text[price.0.range()],
            target = &text[target.0.range()],
        ),
        Command::Open {
            account,
            ..
        } => write!(
            w,
            "{date} open {account}",
            date = &text[date.0.range()],
            account = &text[account.range.range()],
        ),
        Command::Transaction {
            description,
            bookings,
            ..
        } => {
            writeln!(
                w,
                "{date} {description}",
                date = &text[date.0.range()],
                description = &text[description.range.range()]
            )?;
            for b in bookings {
                writeln!(
                    w,
                    "{credit:<width$} {debit:<width$} {amount:>10} {commodity}",
                    credit = &text[b.credit.range.range()],
                    width = n,
                    debit = &text[b.debit.range.range()],
                    amount = &text[b.quantity.0.range()],
                    commodity = &text[b.commodity.0.range()],
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
                date = &text[date.0.range()],
                account = &text[account.range.range()],
                amount = &text[amount.0.range()],
                commodity = &text[commodity.0.range()]
            ),
            _ => {
                writeln!(w, "{date} balance ", date = &text[date.0.range()])?;
                for a in assertions {
                    writeln!(
                        w,
                        "{account} {amount} {commodity}",
                        account = &text[a.account.range.range()],
                        amount = &text[a.amount.0.range()],
                        commodity = &text[a.commodity.0.range()]
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
            date = &text[date.0.range()],
            account = &text[account.range.range()],
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
            interval = &text[interval.range()],
            start = &text[start.0.range()],
            end = &text[end.0.range()],
            account = &text[account.range.range()]
        ),
        Addon::Performance {
            commodities,
            ..
        } => {
            write!(w, "@performance(")?;
            for (i, c) in commodities.iter().enumerate() {
                w.write(&text[c.0.range()].as_bytes())?;
                if i < commodities.len() - 1 {
                    write!(w, ",")?;
                }
            }
            write!(w, ")")
        }
    }
}
