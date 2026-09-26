use std::{
    collections::{HashSet, VecDeque},
    path::Path,
};

use self::{
    cst::{Directive, Include, SyntaxTree},
    error::ParserError,
    sourcefile::SourceFile,
};

pub mod bayes;
pub mod cst;
pub mod error;
pub mod format;
pub mod migrate;
mod parser;
mod scanner;
mod scope;
pub mod sourcefile;

/// Parses a journal held in memory. Unlike [`parse_file`], includes are not
/// followed, since there is no directory to resolve them against.
pub fn parse_text(text: &str) -> std::result::Result<SyntaxTree, ParserError> {
    parser::parse(text).map_err(|e| {
        ParserError::SyntaxError(
            e,
            SourceFile {
                path: None,
                text: text.to_string(),
            },
        )
    })
}

pub fn parse_files(root: &Path) -> std::result::Result<Vec<(SyntaxTree, SourceFile)>, ParserError> {
    let mut res = Vec::new();
    let mut done = HashSet::new();
    let mut todo = VecDeque::new();
    todo.push_back(
        root.canonicalize()
            .map_err(|e| ParserError::IO(root.to_path_buf(), e))?,
    );

    while let Some(file_path) = todo.pop_front() {
        let file =
            SourceFile::read(&file_path).map_err(|e| ParserError::IO(file_path.clone(), e))?;
        let tree =
            parser::parse(&file.text).map_err(|e| ParserError::SyntaxError(e, file.clone()))?;
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

pub fn parse_file(file_path: &Path) -> std::result::Result<(SyntaxTree, SourceFile), ParserError> {
    let file =
        SourceFile::read(file_path).map_err(|e| ParserError::IO(file_path.to_path_buf(), e))?;
    let tree = parser::parse(&file.text).map_err(|e| ParserError::SyntaxError(e, file.clone()))?;
    Ok((tree, file))
}
