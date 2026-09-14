use std::{error::Error, fs, io::Write, path::PathBuf};

use clap::Args;

use crate::syntax::{bayes::Model, format::format_file, parse_file, parse_files, parse_text};

/// The placeholder the importers book counter-postings to.
const TBD_ACCOUNT: &str = "Expenses:TBD";

#[derive(Args)]
pub struct Command {
    /// The journal whose placeholder accounts should be replaced.
    target: PathBuf,

    /// The journal to learn from. Its includes are followed. May be the
    /// target file itself.
    #[arg(short, long)]
    training_file: PathBuf,

    /// The placeholder account to replace.
    #[arg(short, long, default_value = TBD_ACCOUNT)]
    account: String,

    /// Write the result back to the target file instead of to stdout.
    #[arg(short, long)]
    inplace: bool,
}

impl Command {
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        let mut model = Model::new(&self.account);
        for (tree, file) in parse_files(&self.training_file)? {
            model.train(&file.text, &tree);
        }

        let (tree, file) = parse_file(&self.target)?;
        let inferred = crate::syntax::bayes::apply(&file.text, model.infer(&file.text, &tree));

        // The replacements change the width of the account column, so the
        // result is formatted rather than written out verbatim.
        let mut out = Vec::new();
        format_file(&mut out, &inferred, &parse_text(&inferred)?)?;

        if self.inplace {
            fs::write(&self.target, &out)?;
        } else {
            std::io::stdout().lock().write_all(&out)?;
        }
        Ok(())
    }
}
