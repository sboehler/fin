use analyzer::Analyzer;
use error::ModelError;
use journal::Journal;

use crate::syntax::{cst::SyntaxTree, sourcefile::SourceFile};

pub mod entities;
pub mod error;
pub mod journal;
pub mod printing;
pub mod registry;

mod analyzer;
mod prices;

pub fn build_journal(
    trees: &[(SyntaxTree, SourceFile)],
) -> std::result::Result<Journal, ModelError> {
    let mut analyzer = Analyzer::new(registry::Registry::new());
    trees.iter().try_for_each(|(file, source_file)| {
        analyzer
            .analyze(file, source_file)
            .map_err(|e| ModelError::SyntaxError(e, source_file.clone()))
    })?;
    Ok(analyzer.journal())
}
