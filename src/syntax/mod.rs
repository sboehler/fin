use std::{
    collections::{HashSet, VecDeque},
    error::Error,
    path::Path,
};

use self::{
    cst::{Directive, Include, SyntaxFile},
    error::ParserError,
    file::File,
    parser::Parser,
};

pub mod cst;
pub mod error;
pub mod file;
pub mod format;
pub mod parser;
pub mod scanner;

pub fn parse_files(root: &Path) -> std::result::Result<Vec<SyntaxFile>, ParserError> {
    let mut res = Vec::new();
    let mut done = HashSet::new();
    let mut todo = VecDeque::new();
    todo.push_back(
        root.canonicalize()
            .map_err(|e| ParserError::IO(root.to_path_buf(), e))?,
    );

    while let Some(file_path) = todo.pop_front() {
        let file = File::read(&file_path).map_err(|e| ParserError::IO(file_path.clone(), e))?;
        let syntax_file = Parser::new(&file)
            .parse()
            .map_err(ParserError::SyntaxError)?;
        let dir_name = file_path
            .parent()
            .ok_or(ParserError::InvalidPath(file_path.clone()))?;
        for d in &syntax_file.directives {
            if let Directive::Include(Include { path, .. }) = d {
                todo.push_back(
                    dir_name
                        .join(path.content.text())
                        .canonicalize()
                        .map_err(|e| ParserError::IO(file_path.clone(), e))?,
                );
            }
        }
        if !done.insert(file_path.clone()) {
            Err(ParserError::Cycle(file_path.clone()))?;
        }
        res.push(syntax_file);
    }
    Ok(res)
}

pub fn parse_file(file_path: &Path) -> std::result::Result<SyntaxFile, Box<dyn Error>> {
    let file = File::read(file_path).map_err(|e| ParserError::IO(file_path.to_path_buf(), e))?;
    let syntax_tree = Parser::new(&file).parse()?;
    Ok(syntax_tree)
}
