use std::io::{self, Result, Write};

use super::{
    file::File,
    {Addon, Assertion, Directive},
};

pub fn format_file(w: &mut impl Write, file: &File) -> io::Result<()> {
    let n = initialize(file);
    let mut pos = 0;
    for d in &file.syntax_tree.directives {
        w.write_all(file.text[pos..d.range().start].as_bytes())?;
        match d {
            Directive::Include { path, .. } => {
                write!(w, "include {}", file.extract(path.range))?;
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
                    date = file.extract(date.0),
                    commodity = file.extract(commodity.0),
                    price = file.extract(price.0),
                    target = file.extract(target.0),
                )?;
            }
            Directive::Open { date, account, .. } => {
                write!(
                    w,
                    "{date} open {account}",
                    date = file.extract(date.0),
                    account = file.extract(account.range),
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
                    format_addon(w, file, a)?;
                    writeln!(w)?;
                }
                writeln!(
                    w,
                    "{date} {description}",
                    date = file.extract(date.0),
                    description = file.extract(description.range)
                )?;
                for b in bookings {
                    writeln!(
                        w,
                        "{credit:<width$} {debit:<width$} {amount:>10} {commodity}",
                        credit = file.extract(b.credit.range),
                        width = n,
                        debit = file.extract(b.debit.range),
                        amount = file.extract(b.quantity.0),
                        commodity = file.extract(b.commodity.0),
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
                        date = file.extract(date.0),
                        account = file.extract(account.range),
                        amount = file.extract(amount.0),
                        commodity = file.extract(commodity.0)
                    )?,
                    _ => {
                        writeln!(w, "{date} balance ", date = file.extract(date.0))?;
                        for a in assertions {
                            writeln!(
                                w,
                                "{account} {amount} {commodity}",
                                account = file.extract(a.account.range),
                                amount = file.extract(a.balance.0),
                                commodity = file.extract(a.commodity.0)
                            )?;
                        }
                    }
                };
            }
            Directive::Close { date, account, .. } => {
                write!(
                    w,
                    "{date} close {account}",
                    date = file.extract(date.0),
                    account = file.extract(account.range),
                )?;
            }
        }
        pos = d.range().end
    }
    w.write_all(file.text[pos..file.syntax_tree.range.end].as_bytes())?;
    Ok(())
}

fn initialize(f: &File) -> usize {
    f.syntax_tree
        .directives
        .iter()
        .filter_map(|d| match d {
            Directive::Transaction { bookings, .. } => Some(bookings),
            _ => None,
        })
        .flatten()
        .flat_map(|b| vec![&b.credit, &b.debit])
        .map(|a| a.range.slice(&f.text).chars().count())
        .max()
        .unwrap_or_default()
}

fn format_addon(w: &mut impl Write, f: &File, a: &Addon) -> Result<()> {
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
            interval = f.extract(*interval),
            start = f.extract(start.0),
            end = f.extract(end.0),
            account = f.extract(account.range)
        ),
        Addon::Performance { commodities, .. } => {
            write!(w, "@performance(")?;
            for (i, c) in commodities.iter().enumerate() {
                w.write_all(f.extract(c.0).as_bytes())?;
                if i < commodities.len() - 1 {
                    write!(w, ",")?;
                }
            }
            write!(w, ")")
        }
    }
}
