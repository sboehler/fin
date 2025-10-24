use std::{cmp::max, collections::HashMap, fmt::Alignment, io::Write};

use colored::Colorize;
use rust_decimal::Decimal;

#[derive(Debug)]
pub struct Table {
    columns: Vec<usize>,
    rows: Vec<Row>,
}

impl Table {
    pub fn new(groups: Vec<usize>) -> Self {
        Self {
            columns: groups,
            rows: Default::default(),
        }
    }

    pub fn add_row(&mut self, row: Row) {
        self.rows.push(row)
    }
}

#[derive(Debug)]
pub enum Row {
    Row(Vec<Cell>),
    Separator,
    Empty,
}

impl Row {
    pub fn add_cell(&mut self, cell: Cell) {
        match self {
            Self::Row(cells) => cells.push(cell),
            Self::Separator => (),
            Self::Empty => (),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Cell {
    Empty,
    Decimal {
        value: Decimal,
    },
    Text {
        text: String,
        align: Alignment,
        indent: usize,
    },
}

pub struct TextRenderer {
    table: Table,
    round: usize,
}

impl TextRenderer {
    pub fn new(table: Table, round: usize) -> Self {
        Self { table, round }
    }

    pub fn render<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
        let column_widths = self.compute_widths();
        for row in &self.table.rows {
            match row {
                Row::Separator => self.print_separator_row(w, &column_widths)?,
                Row::Row(cells) => self.print_regular_row(w, &column_widths, cells)?,
                Row::Empty => self.print_empty_row(w, &column_widths)?,
            }
        }
        writeln!(w)?;
        Ok(())
    }

    fn print_separator_row<W: Write>(
        &self,
        w: &mut W,
        column_widths: &[usize],
    ) -> std::io::Result<()> {
        write!(w, "+")?;
        for width in column_widths {
            write!(w, "-{}-+", "-".repeat(*width))?;
        }
        writeln!(w)?;
        Ok(())
    }

    fn print_empty_row<W: Write>(&self, w: &mut W, column_widths: &[usize]) -> std::io::Result<()> {
        write!(w, "|")?;
        for width in column_widths {
            write!(w, " {} |", " ".repeat(*width))?;
        }
        writeln!(w)?;
        Ok(())
    }

    fn print_regular_row<W: Write>(
        &self,
        w: &mut W,
        column_widths: &[usize],
        cells: &[Cell],
    ) -> std::io::Result<()> {
        write!(w, "|")?;
        for (i, cell) in cells.iter().enumerate() {
            match cell {
                Cell::Decimal { value } if !value.is_zero() => {
                    let color = match value.is_sign_negative() {
                        true => "red",
                        false => "green",
                    };
                    let formatted = self.format_number(value).color(color);
                    write!(w, " {:>1$} ", formatted, column_widths[i])?
                }
                Cell::Empty | Cell::Decimal { .. } => {
                    write!(w, "{}", " ".repeat(column_widths[i] + 2))?
                }
                Cell::Text {
                    text,
                    align,
                    indent,
                } => {
                    write!(w, " {}", " ".repeat(*indent))?;
                    let width = column_widths[i] - indent;
                    match align {
                        Alignment::Left => write!(w, "{:<1$} ", text, width)?,
                        Alignment::Right => write!(w, "{:>1$} ", text, width)?,
                        Alignment::Center => write!(w, "{:^1$} ", text, width)?,
                    }
                }
            }
            write!(w, "|")?
        }
        writeln!(w)
    }

    fn compute_widths(&self) -> Vec<usize> {
        let mut widths = Vec::new();
        self.table.rows.iter().for_each(|row| match row {
            Row::Row(cells) => {
                if cells.len() > widths.len() {
                    widths.resize(cells.len(), 0)
                }
                cells
                    .iter()
                    .enumerate()
                    .for_each(|(i, cell)| widths[i] = max(widths[i], self.min_length(cell)))
            }
            Row::Separator | Row::Empty => (),
        });
        let mut groups = HashMap::<usize, usize>::new();
        widths.into_iter().enumerate().for_each(|(i, width)| {
            groups
                .entry(self.table.columns[i])
                .and_modify(|group_width| *group_width = max(*group_width, width))
                .or_insert(width);
        });
        self.table
            .columns
            .iter()
            .map(|group_id| groups[group_id])
            .collect()
    }

    fn min_length(&self, c: &Cell) -> usize {
        match c {
            Cell::Empty => 0,
            Cell::Decimal { value } => self.format_number(value).chars().count(),
            Cell::Text { text, indent, .. } => text.len() + indent,
        }
    }

    fn format_number(&self, value: &Decimal) -> String {
        let value = value.round_dp_with_strategy(
            u32::try_from(self.round).unwrap(),
            rust_decimal::RoundingStrategy::MidpointAwayFromZero,
        );
        let text = format!("{value:.0$}", self.round);
        let index = text.find('.').unwrap_or(text.len());
        let mut res = String::new();
        let mut ok = false;
        for (i, ch) in text.chars().enumerate() {
            if i >= index && ch != '-' {
                res.push_str(&text[i..]);
                break;
            }
            if (index - i) % 3 == 0 && ok {
                res.push(',');
            }
            res.push(ch);
            if ch.is_ascii_digit() {
                ok = true;
            }
        }
        res
    }
}
