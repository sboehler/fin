//! A naive Bayes classifier which guesses the account behind a placeholder
//! posting, from the accounts used in transactions that are already assigned.
//!
//! A transaction contributes one observation per booking and direction: for a
//! booking `A B 10 CHF`, both "`A`, given the tokens of this booking" and
//! "`B`, given the tokens" are observed. The tokens are the words of the
//! description plus the commodity, the quantity, and the account on the other
//! side of the booking.
//!
//! Inference picks the account maximising
//! `P(A | T1..Tn) ~ P(A) * P(T1|A) * ... * P(Tn|A)`, computed as a sum of
//! logarithms.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    ops::Range,
};

use super::cst::{Booking, Directive, SyntaxTree, Transaction};

#[derive(Debug)]
pub struct Model {
    /// The placeholder account which inference replaces.
    account: String,
    /// Total number of observations.
    count: usize,
    /// Observations per account. A `BTreeMap` so that candidates are scored
    /// in a fixed order and ties resolve the same way on every run.
    count_by_account: BTreeMap<String, usize>,
    count_by_token_and_account: HashMap<String, HashMap<String, usize>>,
}

impl Model {
    pub fn new(account: &str) -> Self {
        Model {
            account: account.to_string(),
            count: 0,
            count_by_account: BTreeMap::new(),
            count_by_token_and_account: HashMap::new(),
        }
    }

    /// Adds the transactions of a parsed journal to the model. Bookings which
    /// already mention the placeholder account carry no information and are
    /// skipped.
    pub fn train(&mut self, source: &str, tree: &SyntaxTree) {
        for t in transactions(tree) {
            for b in &t.bookings {
                let credit = &source[b.credit.range.clone()];
                let debit = &source[b.debit.range.clone()];
                if credit == self.account || debit == self.account {
                    continue;
                }
                self.observe(source, t, b, credit, debit);
                self.observe(source, t, b, debit, credit);
            }
        }
    }

    fn observe(&mut self, source: &str, t: &Transaction, b: &Booking, account: &str, other: &str) {
        self.count += 1;
        *self
            .count_by_account
            .entry(account.to_string())
            .or_default() += 1;
        for token in tokenize(source, t, b, other) {
            *self
                .count_by_token_and_account
                .entry(token)
                .or_default()
                .entry(account.to_string())
                .or_default() += 1;
        }
    }

    /// Returns the replacements for every occurrence of the placeholder
    /// account, as (range in `source`, inferred account name) pairs.
    pub fn infer(&self, source: &str, tree: &SyntaxTree) -> Vec<(Range<usize>, String)> {
        let mut edits = Vec::new();
        for t in transactions(tree) {
            for b in &t.bookings {
                let credit = &source[b.credit.range.clone()];
                let debit = &source[b.debit.range.clone()];
                if credit == self.account
                    && let Some(account) = self.best(source, t, b, debit)
                {
                    edits.push((b.credit.range.clone(), account));
                }
                if debit == self.account
                    && let Some(account) = self.best(source, t, b, credit)
                {
                    edits.push((b.debit.range.clone(), account));
                }
            }
        }
        edits
    }

    /// The highest scoring account, or `None` if the model has seen no
    /// candidate. `other` is the account on the opposite side of the booking,
    /// which cannot be its own counterpart.
    fn best(&self, source: &str, t: &Transaction, b: &Booking, other: &str) -> Option<String> {
        let tokens = tokenize(source, t, b, other);
        self.count_by_account
            .keys()
            .filter(|candidate| candidate.as_str() != other)
            .map(|candidate| (self.score(candidate, &tokens), candidate))
            .max_by(|(a, _), (b, _)| a.total_cmp(b))
            .map(|(_, candidate)| candidate.clone())
    }

    fn score(&self, candidate: &str, tokens: &HashSet<String>) -> f64 {
        let count = self.count_by_account[candidate] as f64;
        let total = self.count as f64;
        let mut score = (count / total).ln();
        for token in tokens {
            // An unseen token is treated as if it had been observed once
            // across the whole corpus, which penalises but does not exclude
            // the candidate.
            score += match self
                .count_by_token_and_account
                .get(token)
                .and_then(|by_account| by_account.get(candidate))
            {
                Some(&n) => (n as f64 / count).ln(),
                None => (1.0 / total).ln(),
            };
        }
        score
    }
}

/// Applies the replacements returned by [`Model::infer`] to the source text.
pub fn apply(source: &str, mut edits: Vec<(Range<usize>, String)>) -> String {
    edits.sort_by_key(|(range, _)| range.start);
    let mut result = String::with_capacity(source.len());
    let mut pos = 0;
    for (range, account) in edits {
        result.push_str(&source[pos..range.start]);
        result.push_str(&account);
        pos = range.end;
    }
    result.push_str(&source[pos..]);
    result
}

fn transactions(tree: &SyntaxTree) -> impl Iterator<Item = &Transaction> {
    tree.directives.iter().filter_map(|d| match d {
        Directive::Transaction(t) => Some(t),
        _ => None,
    })
}

fn tokenize(source: &str, t: &Transaction, b: &Booking, other: &str) -> HashSet<String> {
    source[t.description.content.clone()]
        .split_whitespace()
        .chain([
            &source[b.commodity.0.clone()],
            &source[b.quantity.0.clone()],
            other,
        ])
        .map(str::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::parse_text;
    use pretty_assertions::assert_eq;

    const TRAINING: &str = r#"2024-01-01 "Migros Zuerich"
Assets:Bank Expenses:Groceries 50.00 CHF

2024-01-02 "Migros Bern"
Assets:Bank Expenses:Groceries 25.00 CHF

2024-01-03 "SBB Ticket"
Assets:Bank Expenses:Travel 10.00 CHF

2024-01-04 "Salary"
Income:Salary Assets:Bank 5000.00 CHF
"#;

    fn model() -> Model {
        let mut model = Model::new("Equity:TBD");
        model.train(TRAINING, &parse_text(TRAINING).unwrap());
        model
    }

    /// Runs inference over `source` and returns the rewritten text.
    fn infer(source: &str) -> String {
        let model = model();
        apply(source, model.infer(source, &parse_text(source).unwrap()))
    }

    #[test]
    fn test_infer() {
        let source = "2024-02-01 \"Migros Zuerich\"\nAssets:Bank Equity:TBD 12.00 CHF\n";
        assert_eq!(
            infer(source),
            "2024-02-01 \"Migros Zuerich\"\nAssets:Bank Expenses:Groceries 12.00 CHF\n"
        );
    }

    /// The description decides between accounts seen with the same counterpart.
    #[test]
    fn test_infer_uses_description() {
        let source = "2024-02-01 \"SBB Ticket\"\nAssets:Bank Equity:TBD 12.00 CHF\n";
        assert_eq!(
            infer(source),
            "2024-02-01 \"SBB Ticket\"\nAssets:Bank Expenses:Travel 12.00 CHF\n"
        );
    }

    /// The placeholder is replaced on whichever side it appears.
    #[test]
    fn test_infer_credit_side() {
        let source = "2024-02-01 \"Salary\"\nEquity:TBD Assets:Bank 5000.00 CHF\n";
        assert_eq!(
            infer(source),
            "2024-02-01 \"Salary\"\nIncome:Salary Assets:Bank 5000.00 CHF\n"
        );
    }

    /// The account on the other side of the booking is never its own counterpart.
    #[test]
    fn test_infer_excludes_other_account() {
        let source = "2024-02-01 \"Migros Zuerich\"\nExpenses:Groceries Equity:TBD 12.00 CHF\n";
        let result = infer(source);
        assert!(
            !result.contains("Expenses:Groceries Expenses:Groceries"),
            "{result}"
        );
    }

    /// Several placeholders in one file are all replaced.
    #[test]
    fn test_infer_multiple_bookings() {
        let source = "2024-02-01 \"Migros Zuerich\"\n\
                      Assets:Bank Equity:TBD 12.00 CHF\n\
                      Assets:Bank Equity:TBD 13.00 CHF\n";
        assert_eq!(
            infer(source),
            "2024-02-01 \"Migros Zuerich\"\n\
             Assets:Bank Expenses:Groceries 12.00 CHF\n\
             Assets:Bank Expenses:Groceries 13.00 CHF\n"
        );
    }

    /// Bookings which do not mention the placeholder are left alone.
    #[test]
    fn test_infer_leaves_assigned_bookings() {
        let source = "2024-02-01 \"Migros Zuerich\"\nAssets:Bank Expenses:Travel 12.00 CHF\n";
        assert_eq!(infer(source), source);
    }

    /// An untrained model has no candidates and changes nothing.
    #[test]
    fn test_infer_without_training() {
        let source = "2024-02-01 \"Migros\"\nAssets:Bank Equity:TBD 12.00 CHF\n";
        let model = Model::new("Equity:TBD");
        assert_eq!(model.infer(source, &parse_text(source).unwrap()), vec![]);
    }

    /// Placeholder bookings in the training file carry no information.
    #[test]
    fn test_train_skips_placeholder() {
        let source = "2024-01-01 \"Migros\"\nAssets:Bank Equity:TBD 50.00 CHF\n";
        let mut model = Model::new("Equity:TBD");
        model.train(source, &parse_text(source).unwrap());
        assert_eq!(model.count, 0);
        assert!(model.count_by_account.is_empty());
    }

    #[test]
    fn test_apply_is_ordered() {
        // Edits are applied left to right regardless of the order given, and
        // may change the length of the text.
        let source = "aa 1 CHF bb";
        let edits = vec![(9..11, "yyy".to_string()), (0..2, "x".to_string())];
        assert_eq!(apply(source, edits), "x 1 CHF yyy");
    }
}
