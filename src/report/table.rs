use std::{cmp::max, collections::HashMap, fmt::Alignment, io::Write};

use rust_decimal::Decimal;

#[derive(Debug)]
pub struct Table {
    pub columns: Vec<usize>,
    pub rows: Vec<Row>,
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
    Row { cells: Vec<Cell> },
    Separator,
}

impl Row {
    pub fn add_cell(&mut self, cell: Cell) {
        match self {
            Self::Row { cells } => cells.push(cell),
            Self::Separator => (),
        }
    }
}

#[derive(Debug)]
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
    pub table: Table,
    pub round: usize,
}

impl TextRenderer {
    pub fn render<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
        let column_widths = self.compute_widths();
        for row in &self.table.rows {
            match row {
                Row::Separator => self.print_separator_row(w, &column_widths)?,
                Row::Row { cells } => self.print_regular_row(w, &column_widths, cells)?,
            }
        }
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

    fn print_regular_row<W: Write>(
        &self,
        w: &mut W,
        column_widths: &[usize],
        cells: &[Cell],
    ) -> std::io::Result<()> {
        write!(w, "|")?;
        for (i, cell) in cells.iter().enumerate() {
            match cell {
                Cell::Empty => write!(w, "{}", " ".repeat(column_widths[i] + 2))?,
                Cell::Decimal { value } => {
                    write!(w, " {:>1$.2$} ", value, column_widths[i], self.round)?
                }
                Cell::Text {
                    text,
                    align,
                    indent,
                } => {
                    write!(w, " {}", " ".repeat(*indent))?;
                    match align {
                        Alignment::Left => write!(w, "{:<1$} ", text, column_widths[i] - indent)?,
                        Alignment::Right => write!(w, "{:>1$} ", text, column_widths[i] - indent)?,
                        Alignment::Center => write!(w, "{:^1$} ", text, column_widths[i] - indent)?,
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
            Row::Row { cells } => {
                if cells.len() > widths.len() {
                    widths.resize(cells.len(), 0)
                }
                cells
                    .iter()
                    .enumerate()
                    .for_each(|(i, cell)| widths[i] = max(widths[i], self.min_length(cell)))
            }
            Row::Separator => (),
        });
        let mut groups = HashMap::<usize, usize>::new();
        widths.into_iter().enumerate().for_each(|(i, width)| {
            let group_id = self.table.columns[i];
            groups
                .entry(group_id)
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
            Cell::Decimal { value } => format!("{value:.0$}", self.round).len(),
            Cell::Text { text, indent, .. } => text.len() + indent,
        }
    }
}
