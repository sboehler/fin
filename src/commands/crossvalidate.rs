//! Held-out prediction over the transactions of a journal.
//!
//! Both `infer --evaluate` and `review` need to ask the model about a
//! transaction it was not trained on: otherwise the transaction teaches the
//! model its own accounts, and the model can never disagree with it.

use crate::syntax::{
    bayes::{Model, transactions},
    cst::{SyntaxTree, Transaction},
    sourcefile::SourceFile,
};

/// One transaction, with the file it came from so that findings can be
/// reported at a location the user can open.
pub struct Item<'a> {
    pub file: &'a SourceFile,
    pub transaction: &'a Transaction,
}

impl Item<'_> {
    pub fn source(&self) -> &str {
        &self.file.text
    }

    /// `path:line` for a byte offset in this file, so a finding points at
    /// somewhere the user can open.
    pub fn location(&self, pos: usize) -> String {
        let (line, _) = self.file.position(pos);
        match &self.file.path {
            Some(path) => format!("{}:{line}", path.display()),
            None => format!("<journal>:{line}"),
        }
    }
}

pub fn items<'a>(files: &'a [(SyntaxTree, SourceFile)]) -> Vec<Item<'a>> {
    files
        .iter()
        .flat_map(|(tree, file)| {
            transactions(tree).map(move |transaction| Item { file, transaction })
        })
        .collect()
}

/// Splits `items` into folds and calls `visit` once per item, with a model
/// trained on every other fold. Returns the number of folds used, which may
/// be fewer than asked for.
pub fn cross_validate(
    items: &[Item],
    account: &str,
    folds: usize,
    mut visit: impl FnMut(&Model, &Item),
) -> usize {
    // At least two folds, and never more than there are transactions to put
    // in them.
    let folds = folds.min(items.len().max(1)).max(2);
    for fold in 0..folds {
        let mut model = Model::new(account);
        for (i, item) in items.iter().enumerate() {
            if i % folds != fold {
                model.train_transaction(item.source(), item.transaction);
            }
        }
        for item in items.iter().skip(fold).step_by(folds) {
            visit(&model, item);
        }
    }
    folds
}
