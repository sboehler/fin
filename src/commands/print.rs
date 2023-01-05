use crate::{context::Context, parser::Parser};
use clap::Args;
use std::{error::Error, fs, path::PathBuf, sync::Arc};

#[derive(Args)]
pub struct Command {
    journal: PathBuf,
}

impl Command {
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        let text = fs::read_to_string(&self.journal)?;
        let mut p =
            Parser::new_from_file(Arc::new(Context::new()), &text, Some(self.journal.clone()));
        let res = p.parse();
        match res {
            Ok(ds) => {
                for d in &ds {
                    println!("{}", d);
                }
            }
            Err(e) => {
                println!("{}", e);
                std::process::exit(1);
            }
        }
        Ok(())
    }
}
