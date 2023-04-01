use crate::journal;
use clap::Args;
use std::{error::Error, path::PathBuf};

#[derive(Args)]
pub struct Command {
    journal: PathBuf,
}

impl Command {
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        match journal::Journal::from_file(self.journal.clone()) {
            Err(e) => println!("{}", e),
            Ok(j) => println!("{} {}", j.min_date().unwrap(), j.max_date().unwrap()),
        }
        Ok(())
    }
}
