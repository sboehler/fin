use std::{
    collections::{HashSet, VecDeque},
    error::Error,
    fmt::Display,
    fs, io,
    path::PathBuf,
};

use super::{
    parser::Parser,
    scanner::ParserError,
    syntax::{Directive, SyntaxTree},
};

pub struct ParsedFile {
    pub file: PathBuf,
    pub text: String,
    pub syntax_tree: SyntaxTree,
}

#[derive(Debug)]
pub enum FileError {
    ParserError(PathBuf, ParserError),
    IO(PathBuf, io::Error),
    Cycle(PathBuf),
    InvalidPath(PathBuf),
}

impl Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileError::ParserError(path, e) => {
                writeln!(
                    f,
                    "error parsing file {file}:",
                    file = path.to_string_lossy()
                )?;
                e.fmt(f)
            }
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

type Result<T> = std::result::Result<T, FileError>;

pub fn parse(root: &PathBuf) -> Result<Vec<ParsedFile>> {
    let mut res = Vec::new();
    let mut done = HashSet::new();
    let mut todo = VecDeque::new();
    todo.push_back(
        root.canonicalize()
            .map_err(|e| FileError::IO(root.clone(), e))?,
    );

    while let Some(file) = todo.pop_front() {
        let text = fs::read_to_string(&file).map_err(|e| FileError::IO(file.clone(), e))?;
        let syntax_tree = Parser::new(&text)
            .parse()
            .map_err(|e| FileError::ParserError(file.clone(), e))?;
        let dir_name = file.parent().ok_or(FileError::InvalidPath(file.clone()))?;
        for d in &syntax_tree.directives {
            if let Directive::Include { path, .. } = d {
                todo.push_back(
                    dir_name
                        .join(path.content.slice(&text))
                        .canonicalize()
                        .map_err(|e| FileError::IO(file.clone(), e))?,
                );
            }
        }
        if !done.insert(file.clone()) {
            return Err(FileError::Cycle(file.clone()));
        }
        res.push(ParsedFile {
            file,
            text,
            syntax_tree,
        });
    }
    Ok(res)
}
