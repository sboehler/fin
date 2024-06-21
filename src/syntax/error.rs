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
pub enum FileError {
    IO(PathBuf, io::Error),
    Cycle(PathBuf),
    InvalidPath(PathBuf),
    SyntaxError(SyntaxError),
}

impl Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileError::IO(path, e) => {
                writeln!(
                    f,
                    "error reading file: {file}:",
                    file = path.to_string_lossy()
                )?;
                e.fmt(f)
            }
            FileError::Cycle(path) => {
                writeln!(
                    f,
                    "cycle detected. File {file} is referenced at least twice",
                    file = path.to_string_lossy()
                )
            }
            FileError::InvalidPath(file) => {
                writeln!(f, "invalid path: {file}", file = file.to_string_lossy())
            }
            FileError::SyntaxError(e) => writeln!(f, "{}", e),
        }
    }
}
