use crate::journal;
use clap::Args;
use std::{error::Error, path::PathBuf};

#[derive(Args)]
pub struct Command {
    journal: PathBuf,
}

impl Command {
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        let (ch, t) = journal::read_from_file(self.journal.clone());
        for v in ch {
            if let Err(e) = v {
                println!("{}", e)
            }
        }
        t.join().unwrap();
        Ok(())
    }
}
