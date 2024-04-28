use crate::syntax::parser::Parser;
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
    let p = Parser::new_from_file(&s, Some(path.clone()));
    p.parse_file()?;
    Ok(())
}
