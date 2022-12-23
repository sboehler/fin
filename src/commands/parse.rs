use crate::journal;
use clap::Args;
use std::{error::Error, path::PathBuf};

#[derive(Args)]
pub struct Command {
    journal: PathBuf,
}

impl Command {
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        let ch = journal::read_from_file(self.journal.clone());
        for v in ch {
            match v {
                Err(e) => println!("{}", e),
                Ok(_) => (),
            }
        }
        Ok(())
    }
}
