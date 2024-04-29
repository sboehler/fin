use crate::syntax::{parser::Parser, syntax::SourceFile};
use clap::Args;
use std::{error::Error, fs, path::PathBuf};

#[derive(Args)]
pub struct Command {
    journal: PathBuf,
}

impl Command {
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        if let Err(e) = execute(self.journal.clone()) {
            println!("{}", e);
            std::process::exit(1)
        }
        Ok(())
    }
}

fn execute(path: PathBuf) -> Result<(), Box<dyn Error>> {
    let s = fs::read_to_string(&path.clone())?;
    parse_file(path.clone(), &s)?;
    Ok(())
}

fn parse_file<'a>(
    path: PathBuf,
    s: &'a str,
) -> Result<SourceFile<'a>, Box<dyn Error>> {
    let p = Parser::new_from_file(&s, Some(path.clone()));
    let sf = p.parse_file()?;
    Ok(sf)
}
