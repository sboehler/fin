use std::{io::Write, rc::Rc};

use super::{
    entities::{Assertion, Price, Transaction},
    registry::Registry,
};

pub struct Printer<'a, W: Write> {
    registry: Rc<Registry>,
    writer: &'a mut W,
}

impl<'a, W: Write> Printer<'a, W> {
    pub fn new(writer: &'a mut W, registry: Rc<Registry>) -> Self {
        Self { registry, writer }
    }

    pub fn price(&mut self, p: &Price) -> std::io::Result<()> {
        writeln!(
            self.writer,
            "{date} price {commodity} {price} {target}",
            date = p.date,
            commodity = self.registry.commodity_name(p.commodity),
            price = p.price,
            target = self.registry.commodity_name(p.target),
        )
    }

    pub fn newline(&mut self) -> std::io::Result<()> {
        writeln!(self.writer)
    }

    pub fn assertion(&mut self, a: &Assertion) -> std::io::Result<()> {
        writeln!(
            self.writer,
            "{date} balance {account} {balance} {commodity}",
            date = a.date,
            account = self.registry.account_name(a.account),
            balance = a.balance,
            commodity = self.registry.commodity_name(a.commodity),
        )
    }

    /// Prints the transactions separated by blank lines, with account
    /// columns aligned across all of them.
    pub fn transactions(&mut self, ts: &[Transaction]) -> std::io::Result<()> {
        let width = ts
            .iter()
            .flat_map(|t| &t.bookings)
            .map(|b| self.registry.account_name(b.account).chars().count())
            .max()
            .unwrap_or_default();
        for (i, t) in ts.iter().enumerate() {
            if i > 0 {
                self.newline()?;
            }
            self.transaction(t, width)?;
        }
        Ok(())
    }

    pub fn transaction(&mut self, t: &Transaction, width: usize) -> std::io::Result<()> {
        if let Some(targets) = &t.targets {
            let names = targets
                .iter()
                .map(|c| self.registry.commodity_name(*c))
                .collect::<Vec<_>>();
            writeln!(self.writer, "@performance({})", names.join(","))?;
        }
        writeln!(
            self.writer,
            "{date} \"{description}\"",
            date = t.date,
            description = t.description
        )?;
        // Bookings come in (credit, debit) pairs as produced by
        // Booking::create; one line is printed per pair, from the debit side.
        for b in t.bookings.iter().skip(1).step_by(2) {
            writeln!(
                self.writer,
                "{credit:<width$} {debit:<width$} {amount:>10} {commodity}",
                credit = self.registry.account_name(b.other),
                debit = self.registry.account_name(b.account),
                amount = b.quantity,
                commodity = self.registry.commodity_name(b.commodity),
            )?;
        }
        Ok(())
    }
}
