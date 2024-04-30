use crate::syntax::{format::format_file, parser::Parser, syntax::SourceFile};
use clap::Args;
use std::{error::Error, fs, path::PathBuf};

#[derive(Args)]
pub struct Command {
    file: Vec<PathBuf>,
}

impl Command {
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        self.file.iter().map(execute).collect()
    }
}

fn execute(path: &PathBuf) -> Result<(), Box<dyn Error>> {
    let s = fs::read_to_string(path)?;
    let f = parse_file(path, &s)?;
    let mut b = Vec::new();
    format_file(&mut b, &f)?;
    fs::write(path, b)?;
    Ok(())
}

fn parse_file<'a>(
    path: &'a PathBuf,
    s: &'a str,
) -> Result<SourceFile<'a>, Box<dyn Error>> {
    let p = Parser::new_from_file(&s, Some(&path));
    let sf = p.parse_file()?;
    Ok(sf)
}
