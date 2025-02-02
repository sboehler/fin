use std::{
    collections::{HashSet, VecDeque},
    error::Error,
    path::Path,
};

use self::{
    cst::{Directive, Include, SyntaxTree},
    error::ParserError,
    file::File,
    parser::Parser,
};

pub mod cst;
pub mod error;
pub mod file;
pub mod format;
mod parser;
mod scanner;

pub fn parse_files(root: &Path) -> std::result::Result<Vec<(SyntaxTree, File)>, ParserError> {
    let mut res = Vec::new();
    let mut done = HashSet::new();
    let mut todo = VecDeque::new();
    todo.push_back(
        root.canonicalize()
            .map_err(|e| ParserError::IO(root.to_path_buf(), e))?,
    );

    while let Some(file_path) = todo.pop_front() {
        let file = File::read(&file_path).map_err(|e| ParserError::IO(file_path.clone(), e))?;
        let tree = Parser::new(&file.text)
            .parse()
            .map_err(|e| ParserError::SyntaxError(e, file.clone()))?;
        let dir_name = file_path
            .parent()
            .ok_or(ParserError::InvalidPath(file_path.clone()))?;
        for d in &tree.directives {
            if let Directive::Include(Include { path, .. }) = d {
                todo.push_back(
                    dir_name
                        .join(&file.text[path.content.clone()])
                        .canonicalize()
                        .map_err(|e| ParserError::IO(file_path.clone(), e))?,
                );
            }
        }
        if !done.insert(file_path.clone()) {
            Err(ParserError::Cycle(file_path.clone()))?;
        }
        res.push((tree, file));
    }
    Ok(res)
}

pub fn parse_file(file_path: &Path) -> std::result::Result<(SyntaxTree, File), Box<dyn Error>> {
    let file = File::read(file_path).map_err(|e| ParserError::IO(file_path.to_path_buf(), e))?;
    let tree = Parser::new(&file.text).parse()?;
    Ok((tree, file))
}
