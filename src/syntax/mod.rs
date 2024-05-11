use std::{
    collections::{HashSet, VecDeque},
    error::Error,
    path::Path,
};

use self::{
    cst::{Directive, SyntaxTree},
    error::FileError,
    file::File,
    parser::Parser,
};

pub mod cst;
pub mod error;
pub mod file;
pub mod format;
pub mod parser;
pub mod parser2;
pub mod scanner;
pub mod scanner2;

pub fn parse_files(root: &Path) -> std::result::Result<Vec<SyntaxTree>, Box<dyn Error>> {
    let mut res = Vec::new();
    let mut done = HashSet::new();
    let mut todo = VecDeque::new();
    todo.push_back(
        root.canonicalize()
            .map_err(|e| FileError::IO(root.to_path_buf(), e))?,
    );

    while let Some(file_path) = todo.pop_front() {
        let file = File::read(&file_path).map_err(|e| FileError::IO(file_path.clone(), e))?;
        let syntax_tree = Parser::new(&file).parse()?;
        let dir_name = file_path
            .parent()
            .ok_or(FileError::InvalidPath(file_path.clone()))?;
        for d in &syntax_tree.directives {
            if let Directive::Include { path, .. } = d {
                todo.push_back(
                    dir_name
                        .join(path.content.text())
                        .canonicalize()
                        .map_err(|e| FileError::IO(file_path.clone(), e))?,
                );
            }
        }
        if !done.insert(file_path.clone()) {
            Err(FileError::Cycle(file_path.clone()))?;
        }
        res.push(syntax_tree);
    }
    Ok(res)
}

pub fn parse_file(file_path: &Path) -> std::result::Result<SyntaxTree, Box<dyn Error>> {
    let file = File::read(file_path).map_err(|e| FileError::IO(file_path.to_path_buf(), e))?;
    let syntax_tree = Parser::new(&file).parse()?;
    Ok(syntax_tree)
}
