use crate::journal;
use clap::Args;
use std::{error::Error, path::PathBuf};

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
    let j = journal::Journal::from_file(path)?;
    println!("{} {}", j.min_date().unwrap(), j.max_date().unwrap());
    Ok(())
}
