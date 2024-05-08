use std::{
    collections::{HashSet, VecDeque},
    error::Error,
    fs, io,
    path::{Path, PathBuf},
};

use super::{
    cst::{Directive, Rng, SyntaxTree},
    error::FileError,
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

pub fn parse_files(root: &Path) -> std::result::Result<Vec<File>, Box<dyn Error>> {
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
        let syntax_tree = Parser::new(&text).parse()?;
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
            Err(FileError::Cycle(file_path.clone()))?;
        }
        res.push(File {
            path: file_path,
            text,
            syntax_tree,
        });
    }
    Ok(res)
}

pub fn parse_file(file: &Path) -> std::result::Result<File, Box<dyn Error>> {
    let file = file
        .canonicalize()
        .map_err(|e| FileError::IO(file.to_path_buf(), e))?;
    let text = fs::read_to_string(&file).map_err(|e| FileError::IO(file.clone(), e))?;
    let syntax_tree = Parser::new(&text).parse()?;
    Ok(File {
        path: file,
        text,
        syntax_tree,
    })
}
