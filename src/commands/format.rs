use crate::syntax::{format::format_file, parse_file};
use clap::Args;
use std::{error::Error, fs, path::PathBuf};

#[derive(Args)]
pub struct Command {
    file: Vec<PathBuf>,
}

impl Command {
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        self.file.iter().try_for_each(execute)
    }
}

fn execute(path: &PathBuf) -> Result<(), Box<dyn Error>> {
    let (syntax_tree, file) = parse_file(path)?;
    let mut w = Vec::new();
    format_file(&mut w, &file.text, &syntax_tree)?;
    fs::write(path, &w)?;
    Ok(())
}
