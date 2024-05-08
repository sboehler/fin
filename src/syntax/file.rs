use std::{
    collections::{HashSet, VecDeque},
    error::Error,
    fmt::Display,
    fs, io,
    path::{Path, PathBuf},
};

use super::{
    cst::{Directive, Rng, SyntaxTree},
    error::SyntaxError,
    parser::Parser,
};

pub struct File {
    pub path: PathBuf,
    pub text: String,
    pub syntax_tree: SyntaxTree,
}

impl File {
    pub fn extract(&self, rng: Rng) -> &str {
        rng.slice(&self.text)
    }
}

#[derive(Debug)]
pub enum FileError {
    SyntaxError(PathBuf, SyntaxError),
    IO(PathBuf, io::Error),
    Cycle(PathBuf),
    InvalidPath(PathBuf),
}

impl Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileError::SyntaxError(path, e) => {
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

pub fn parse_files(root: &Path) -> std::result::Result<Vec<File>, FileError> {
    let mut res = Vec::new();
    let mut done = HashSet::new();
    let mut todo = VecDeque::new();
    todo.push_back(
        root.canonicalize()
            .map_err(|e| FileError::IO(root.to_path_buf(), e))?,
    );

    while let Some(file_path) = todo.pop_front() {
        let text =
            fs::read_to_string(&file_path).map_err(|e| FileError::IO(file_path.clone(), e))?;
        let syntax_tree = Parser::new(&text)
            .parse()
            .map_err(|e| FileError::SyntaxError(file_path.clone(), e))?;
        let dir_name = file_path
            .parent()
            .ok_or(FileError::InvalidPath(file_path.clone()))?;
        for d in &syntax_tree.directives {
            if let Directive::Include { path, .. } = d {
                todo.push_back(
                    dir_name
                        .join(path.content.slice(&text))
                        .canonicalize()
                        .map_err(|e| FileError::IO(file_path.clone(), e))?,
                );
            }
        }
        if !done.insert(file_path.clone()) {
            return Err(FileError::Cycle(file_path.clone()));
        }
        res.push(File {
            path: file_path,
            text,
            syntax_tree,
        });
    }
    Ok(res)
}

pub fn parse_file(file: &Path) -> std::result::Result<File, FileError> {
    let file = file
        .canonicalize()
        .map_err(|e| FileError::IO(file.to_path_buf(), e))?;
    let text = fs::read_to_string(&file).map_err(|e| FileError::IO(file.clone(), e))?;
    let syntax_tree = Parser::new(&text)
        .parse()
        .map_err(|e| FileError::SyntaxError(file.clone(), e))?;
    Ok(File {
        path: file,
        text,
        syntax_tree,
    })
}
