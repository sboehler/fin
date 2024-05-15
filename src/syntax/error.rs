use std::{error::Error, fmt::Display, io, path::PathBuf};

use thiserror::Error;

use super::cst::{Rng, Token};

#[derive(Error, Debug, Eq, PartialEq)]
pub struct SyntaxError {
    pub rng: Rng,
    pub want: Token,
    pub source: Option<Box<SyntaxError>>,
}

impl SyntaxError {
    fn position(t: &str, pos: usize) -> (usize, usize) {
        let lines: Vec<_> = t[..pos].split(|c| c == '\n').collect();
        let line = lines.len();
        let col = lines.last().iter().flat_map(|s| s.chars()).count() + 1;
        (line, col)
    }
}

impl std::fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let (line, col) = Self::position(&self.rng.file.text, self.rng.start);
        let start = line.saturating_sub(4);
        let context = self
            .rng
            .file
            .text
            .lines()
            .enumerate()
            .skip(start)
            .take(line - start)
            .map(|(i, l)| (i + 1, l.to_string()))
            .collect::<Vec<(usize, String)>>();
        writeln!(f)?;
        write!(
            f,
            "Line {line}, column {col}: while parsing {want}",
            line = line,
            col = col,
            want = self.want
        )?;
        writeln!(f)?;
        writeln!(f)?;

        for (n, line) in context {
            writeln!(f, "{:5} |{}", n, line)?;
        }
        writeln!(f, "{}^ want {}", " ".repeat(col + 6), self.want,)?;
        writeln!(f)?;
        if let Some(e) = &self.source {
            writeln!(f, "{}", e)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_parser_error {
    use crate::syntax::file::File;

    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_parser_error() {
        assert_eq!(
            [
                "",
                "Line 0, column 1: while parsing source file",
                "",
                "    0|asdf",
                "       ^ want source file, got 's'",
                ""
            ]
            .join("\n"),
            SyntaxError {
                want: Token::File,
                rng: Rng::new(File::mem("asdf"), 1, 2),
                source: None,
            }
            .to_string()
        );
        assert_eq!(SyntaxError::position("foo\nbar\n", 0), (0, 0));
        assert_eq!(SyntaxError::position("foo\nbar\n", 1), (0, 1));
        assert_eq!(SyntaxError::position("foo\nbar\n", 2), (0, 2));
        assert_eq!(SyntaxError::position("foo\nbar\n", 3), (0, 3));
        assert_eq!(SyntaxError::position("foo\nbar\n", 4), (1, 0));
        assert_eq!(SyntaxError::position("foo\nbar\n", 5), (1, 1));
        assert_eq!(SyntaxError::position("foo\nbar\n", 6), (1, 2));
        assert_eq!(SyntaxError::position("foo\nbar\n", 7), (1, 3));
        assert_eq!(SyntaxError::position("foo\nbar\n", 8), (2, 0));
    }
}

#[derive(Debug)]
pub enum FileError {
    IO(PathBuf, io::Error),
    Cycle(PathBuf),
    InvalidPath(PathBuf),
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
        }
    }
}

impl Error for FileError {}
