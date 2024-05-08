use std::{error::Error, fmt::Display, io, path::PathBuf};

use super::cst::Token;

#[derive(Debug, Eq, PartialEq)]
pub struct SyntaxError {
    got: Token,
    want: Token,
    msg: Option<String>,
    line: usize,
    col: usize,
    context: Vec<(usize, String)>,
}

impl SyntaxError {
    pub fn new(
        source: &str,
        pos: usize,
        msg: Option<String>,
        want: Token,
        got: Token,
    ) -> SyntaxError {
        let (line, col) = Self::position(source, pos);
        let rng = line.saturating_sub(4)..=line;
        let context = source
            .lines()
            .enumerate()
            .filter(|t| rng.contains(&t.0))
            .map(|(i, l)| (i, l.into()))
            .collect();
        SyntaxError {
            line,
            col,
            context,
            msg,
            want,
            got,
        }
    }

    fn position(t: &str, pos: usize) -> (usize, usize) {
        let lines: Vec<_> = t[..pos].split(|c| c == '\n').collect();
        let line = lines.len().saturating_sub(1);
        let col = lines.last().map(|s| s.len()).unwrap_or(0);
        (line, col)
    }

    pub fn update(mut self, msg: &str) -> Self {
        self.msg = Some(msg.into());
        self
    }
}

impl std::fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        writeln!(f)?;
        write!(
            f,
            "Line {line}, column {col}:",
            line = self.line,
            col = self.col,
        )?;
        if let Some(ref s) = self.msg {
            writeln!(f, " while {}", s)?;
        } else {
            writeln!(f)?;
        }
        writeln!(f)?;
        for (n, line) in &self.context {
            writeln!(f, "{:5}|{}", n, line)?;
        }
        writeln!(
            f,
            "{}^ want {}, got {}",
            " ".repeat(self.col + 6),
            self.want,
            self.got
        )?;
        if let Token::Error(ref e) = self.got {
            e.fmt(f)?;
        }
        Ok(())
    }
}

impl std::error::Error for SyntaxError {}

#[cfg(test)]
mod test_parser_error {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_parser_error() {
        assert_eq!(
            [
                "",
                "Line 0, column 1: while parsing file",
                "",
                "    0|asdf",
                "       ^ want whitespace, got a character (a-z, A-Z) or a digit (0-9)",
                ""
            ]
            .join("\n"),
            SyntaxError {
                got: Token::AlphaNum,
                want: Token::WhiteSpace,
                msg: Some("parsing file".into()),
                line: 0,
                col: 1,
                context: vec![(0, "asdf".into())]
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
                    "error reading file {file}:",
                    file = path.to_string_lossy()
                )?;
                e.fmt(f)
            }
            FileError::Cycle(path) => {
                writeln!(
                    f,
                    "error: cycle detected. File {file} is referenced at least twice",
                    file = path.to_string_lossy()
                )
            }
            FileError::InvalidPath(file) => {
                writeln!(
                    f,
                    "error: invalid path {file}",
                    file = file.to_string_lossy()
                )
            }
        }
    }
}

impl Error for FileError {}
