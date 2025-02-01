use std::{fmt::Display, io, path::PathBuf};

use thiserror::Error;

use super::cst::{Rng, Token};

#[derive(Error, Debug, Eq, PartialEq)]
pub struct SyntaxError {
    pub rng: Rng,
    pub want: Token,
    pub source: Option<Box<SyntaxError>>,
}

impl std::fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let (line, col) = self.rng.file.position(self.rng.start);
        writeln!(f)?;
        if let Some(p) = &self.rng.file.path {
            writeln!(f, "In file \"{}\"", p.to_string_lossy())?;
        }
        write!(
            f,
            "Line {line}, column {col}: while parsing {want}",
            want = self.want
        )?;
        writeln!(f)?;
        writeln!(f)?;

        for (n, line) in self.rng.context() {
            writeln!(f, "{n:5} |{line}")?;
        }
        writeln!(f, "{}^ want {}", " ".repeat(col + 6), self.want,)?;
        writeln!(f)?;
        if let Some(e) = &self.source {
            writeln!(f, "{}", e)?;
        }
        Ok(())
    }
}

#[derive(Error, Debug)]
pub enum ParserError {
    IO(PathBuf, io::Error),
    Cycle(PathBuf),
    InvalidPath(PathBuf),
    SyntaxError(SyntaxError),
}

impl Display for ParserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParserError::IO(path, e) => {
                let file = path.to_string_lossy();
                writeln!(f, "error reading file: {file}:")?;
                e.fmt(f)
            }
            ParserError::Cycle(path) => {
                let file = path.to_string_lossy();
                writeln!(f, "cycle detected: {file} is referenced at least twice")
            }
            ParserError::InvalidPath(file) => {
                let file = file.to_string_lossy();
                writeln!(f, "invalid path: {file}")
            }
            ParserError::SyntaxError(e) => writeln!(f, "{}", e),
        }
    }
}
