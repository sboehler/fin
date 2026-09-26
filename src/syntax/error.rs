use std::{fmt::Display, io, ops::Range, path::PathBuf};

use thiserror::Error;

use super::{cst::Token, sourcefile::SourceFile};

/// What the parser wanted and did not get, and the productions it was in the
/// middle of when that happened. The head is the failure itself; `context`
/// widens one production at a time, out to the directive it began with.
#[derive(Error, Debug, Eq, PartialEq)]
pub struct SyntaxError {
    pub range: Range<usize>,
    pub want: Token,
    pub context: Option<Box<SyntaxError>>,
}

impl std::fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "syntax error: expected {}", self.want)?;
        Ok(())
    }
}

impl SyntaxError {
    /// The failure, pointed at in its line, and then the productions it
    /// happened inside, innermost first.
    pub fn full_error(&self, f: &mut std::fmt::Formatter, file: &SourceFile) -> std::fmt::Result {
        let (line, col) = file.position(self.range.start);
        writeln!(f)?;
        if let Some(p) = &file.path {
            writeln!(f, "In file \"{}\"", p.to_string_lossy())?;
        }
        writeln!(f, "Line {line}, column {col}:")?;
        writeln!(f)?;
        file.fmt_range(f, &self.range)?;
        writeln!(f, "{}^ want {}", " ".repeat(col + 6), self.want)?;
        let mut context = self.context.as_deref();
        if context.is_some() {
            writeln!(f)?;
        }
        while let Some(e) = context {
            let (line, col) = file.position(e.range.start);
            writeln!(
                f,
                "  while parsing {want}, from line {line}, column {col}",
                want = e.want
            )?;
            context = e.context.as_deref();
        }
        writeln!(f)
    }
}

#[derive(Error, Debug)]
pub enum ParserError {
    IO(PathBuf, io::Error),
    Cycle(PathBuf),
    InvalidPath(PathBuf),
    SyntaxError(SyntaxError, SourceFile),
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
            ParserError::SyntaxError(error, file) => {
                writeln!(f, "{error}")?;
                error.full_error(f, file)
            }
        }
    }
}
