use std::{fmt::Alignment, iter::repeat};

use rust_decimal::Decimal;

pub struct Table {
    pub columns: Vec<usize>,
    pub rows: Vec<Row>,
}

impl Table {
    pub fn new(groups: &[usize]) -> Self {
        Self {
            columns: groups
                .iter()
                .enumerate()
                .flat_map(|(i, size)| repeat(i).take(*size))
                .collect(),
            rows: Default::default(),
        }
    }

    pub fn add_row(&mut self, row: Row) {
        self.rows.push(row)
    }
}

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
