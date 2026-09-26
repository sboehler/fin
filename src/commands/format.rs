use crate::syntax::{format::format_file, parse_file};
use clap::Args;
use std::{error::Error, fs, io::Write, path::PathBuf};

/// Rewrites journals in the canonical layout.
#[derive(Args)]
pub struct Command {
    /// The journals to format. Includes are not followed: name every file.
    #[arg(required = true)]
    file: Vec<PathBuf>,

    /// Print the result to stdout instead of writing it back to the file.
    #[arg(short = 'n', long)]
    dry_run: bool,
}

impl Command {
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        self.file.iter().try_for_each(|path| self.execute(path))
    }

    fn execute(&self, path: &PathBuf) -> Result<(), Box<dyn Error>> {
        let (syntax_tree, file) = parse_file(path)?;
        let mut out = Vec::new();
        format_file(&mut out, &file.text, &syntax_tree)?;

        if self.dry_run {
            std::io::stdout().lock().write_all(&out)?;
        } else {
            fs::write(path, &out)?;
        }
        Ok(())
    }
}
