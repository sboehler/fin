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
    /// Text, which may hold line breaks: the cell is then as many lines
    /// tall, and the row grows to fit its tallest cell.
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
        for line in 0..Self::height(cells) {
            write!(w, "|")?;
            for (i, cell) in cells.iter().enumerate() {
                self.print_cell(w, column_widths[i], cell, line)?;
                write!(w, "|")?
            }
            writeln!(w)?;
        }
        Ok(())
    }

    /// The number of lines the row occupies, which is the number of lines of
    /// its tallest cell.
    fn height(cells: &[Cell]) -> usize {
        cells
            .iter()
            .map(|cell| match cell {
                Cell::Text { text, .. } => text.lines().count(),
                Cell::Empty | Cell::Decimal { .. } => 1,
            })
            .max()
            .unwrap_or_default()
            // An empty cell still takes a line.
            .max(1)
    }

    /// Prints the `line`th line of `cell`, padded to `width`. Cells which
    /// have no such line are blank there.
    fn print_cell<W: Write>(
        &self,
        w: &mut W,
        width: usize,
        cell: &Cell,
        line: usize,
    ) -> std::io::Result<()> {
        match cell {
            Cell::Decimal { value } if line == 0 && !value.is_zero() => {
                let color = match value.is_sign_negative() {
                    true => "red",
                    false => "green",
                };
                let formatted = self.format_number(value).color(color);
                write!(w, " {formatted:>width$} ")
            }
            Cell::Empty | Cell::Decimal { .. } => write!(w, "{}", " ".repeat(width + 2)),
            Cell::Text {
                text,
                align,
                indent,
            } => {
                let text = text.lines().nth(line).unwrap_or("");
                write!(w, " {}", " ".repeat(*indent))?;
                let width = width - indent;
                match align {
                    Alignment::Left => write!(w, "{text:<width$} "),
                    Alignment::Right => write!(w, "{text:>width$} "),
                    Alignment::Center => write!(w, "{text:^width$} "),
                }
            }
        }
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
            Cell::Text { text, indent, .. } => {
                // The widest line, since the cell is as wide as its longest
                // one. Padding counts characters, not bytes.
                text.lines()
                    .map(|line| line.chars().count())
                    .max()
                    .unwrap_or(0)
                    + indent
            }
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
        for (i, ch) in text.char_indices() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn text(text: &str) -> Cell {
        Cell::Text {
            text: text.to_string(),
            align: Alignment::Left,
            indent: 0,
        }
    }

    fn render(table: Table) -> String {
        // The amounts would otherwise be colored when rendering to a
        // terminal.
        colored::control::set_override(false);
        let mut out = Vec::new();
        TextRenderer::new(table, 2).render(&mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    /// A cell with line breaks makes the row as tall as the cell, and the
    /// cells beside it are blank below their first line. The column is as
    /// wide as the cell's longest line.
    #[test]
    fn test_renders_a_multiline_cell() {
        let mut table = Table::new(vec![0, 1]);
        table.add_row(Row::Separator);
        table.add_row(Row::Row(vec![
            text("one\ntwo three\nfour"),
            Cell::Decimal {
                value: Decimal::new(-1234, 1),
            },
        ]));
        table.add_row(Row::Separator);
        assert_eq!(
            render(table),
            "+-----------+---------+\n\
             | one       | -123.40 |\n\
             | two three |         |\n\
             | four      |         |\n\
             +-----------+---------+\n\n"
        );
    }

    /// A column is as wide as its widest character count, not byte count.
    #[test]
    fn test_column_width_counts_characters() {
        let mut table = Table::new(vec![0]);
        table.add_row(Row::Row(vec![text("Zürich")]));
        assert_eq!(render(table), "| Zürich |\n\n");
    }
}
