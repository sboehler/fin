use crate::parser::parse;
use crate::scanner::Scanner;
use clap::Args;
use std::{error::Error, fs, path::PathBuf};

#[derive(Args)]
pub struct Command {
    journal: PathBuf,
}

impl Command {
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        let text = fs::read_to_string(&self.journal)?;
        let mut p = Scanner::new(&text);
        let ds = parse(&mut p)?;
        for d in &ds {
            println!("{}", d);
            println!();
        }
        Ok(())
    }
}
