use std::{error::Error, fs, io::Write, path::PathBuf};

use clap::Args;

use crate::syntax::{
    format::format_file,
    migrate::{DEFAULT_WIDTH, migrate},
    parse_file, parse_text,
};

/// Rewrites the transactions of a journal from the old notation, one booking
/// per line, into the arrow notation.
#[derive(Args)]
pub struct Command {
    /// The journals to rewrite. Includes are not followed: name every file.
    #[arg(required = true)]
    file: Vec<PathBuf>,

    /// Wrap descriptions, which the arrow notation writes unquoted below the
    /// date, to this many columns.
    #[arg(short, long, default_value_t = DEFAULT_WIDTH)]
    width: usize,

    /// Print the result to stdout instead of writing it back to the file.
    #[arg(short = 'n', long)]
    dry_run: bool,
}

impl Command {
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        self.file.iter().try_for_each(|path| self.execute(path))
    }

    fn execute(&self, path: &PathBuf) -> Result<(), Box<dyn Error>> {
        let (tree, file) = parse_file(path)?;
        let migrated = migrate(&file.text, &tree, self.width);

        // Migrating moves the accounts into other columns, so the result is
        // formatted rather than written out verbatim.
        let mut out = Vec::new();
        format_file(&mut out, &migrated, &parse_text(&migrated)?)?;

        if self.dry_run {
            std::io::stdout().lock().write_all(&out)?;
        } else {
            fs::write(path, &out)?;
        }
        Ok(())
    }
}
