use std::{io::Write, rc::Rc};

use super::{entities::Price, registry::Registry};

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
}
