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

/// The account the model would put in place of a placeholder.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub account: String,
    /// Share of the posterior falling on this account, between 0 and 1. Naive
    /// Bayes is overconfident by construction, so this separates "no evidence"
    /// from "some evidence" well but should not be read as a calibrated
    /// probability.
    pub confidence: f64,
}

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
    /// Observations containing each token, i.e. its document frequency.
    count_by_token: HashMap<String, usize>,
}

/// A token appearing in more than this share of observations is ignored when
/// scoring: it is present whatever the account, so it cannot tell candidates
/// apart. Measured by `infer --evaluate` on a 13k transaction journal, where
/// 0.1 was the best of 1.0, 0.5, 0.3, 0.2, 0.1 and 0.05.
const MAX_DOCUMENT_FREQUENCY: f64 = 0.1;

/// ... but only once a token has been seen often enough for its frequency to
/// mean anything, so that small journals are not stripped of all evidence.
const MIN_OCCURRENCES_TO_PRUNE: usize = 20;

impl Model {
    pub fn new(account: &str) -> Self {
        Model {
            account: account.to_string(),
            count: 0,
            count_by_account: BTreeMap::new(),
            count_by_token_and_account: HashMap::new(),
            count_by_token: HashMap::new(),
        }
    }

    /// Adds the transactions of a parsed journal to the model.
    pub fn train(&mut self, source: &str, tree: &SyntaxTree) {
        for t in transactions(tree) {
            self.train_transaction(source, t);
        }
    }

    /// Adds a single transaction, so that callers holding their own selection
    /// of transactions (cross-validation folds) can train on it. Bookings
    /// which already mention the placeholder account carry no information and
    /// are skipped.
    pub fn train_transaction(&mut self, source: &str, t: &Transaction) {
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

    fn observe(&mut self, source: &str, t: &Transaction, b: &Booking, account: &str, other: &str) {
        self.count += 1;
        *self
            .count_by_account
            .entry(account.to_string())
            .or_default() += 1;
        for token in tokenize(source, t, b, other) {
            *self.count_by_token.entry(token.clone()).or_default() += 1;
            *self
                .count_by_token_and_account
                .entry(token)
                .or_default()
                .entry(account.to_string())
                .or_default() += 1;
        }
    }

    /// Returns the replacements for every occurrence of the placeholder
    /// account, as (range in `source`, inferred account name) pairs. Bookings
    /// whose best candidate does not reach `min_confidence` are left alone, so
    /// that a guess the model is unsure about stays visible as a placeholder
    /// instead of becoming a plausible-looking wrong account.
    pub fn infer(
        &self,
        source: &str,
        tree: &SyntaxTree,
        min_confidence: f64,
    ) -> Vec<(Range<usize>, String)> {
        let mut edits = Vec::new();
        for t in transactions(tree) {
            for b in &t.bookings {
                let credit = &source[b.credit.range.clone()];
                let debit = &source[b.debit.range.clone()];
                if credit == self.account
                    && let Some(c) = self.predict(source, t, b, debit)
                    && c.confidence >= min_confidence
                {
                    edits.push((b.credit.range.clone(), c.account));
                }
                if debit == self.account
                    && let Some(c) = self.predict(source, t, b, credit)
                    && c.confidence >= min_confidence
                {
                    edits.push((b.debit.range.clone(), c.account));
                }
            }
        }
        edits
    }

    /// The highest scoring account for the side of `b` opposite to `other`,
    /// or `None` if the model has seen no candidate. `other` is the account on
    /// the other side of the booking, which cannot be its own counterpart.
    pub fn predict(
        &self,
        source: &str,
        t: &Transaction,
        b: &Booking,
        other: &str,
    ) -> Option<Candidate> {
        let tokens = tokenize(source, t, b, other)
            .into_iter()
            .filter(|token| !self.is_ubiquitous(token))
            .collect::<HashSet<_>>();
        let mut scored = self
            .count_by_account
            .keys()
            .filter(|candidate| candidate.as_str() != other)
            .map(|candidate| (self.score(candidate, &tokens), candidate))
            .collect::<Vec<_>>();
        // Descending. The sort is stable and the candidates arrive in
        // alphabetical order, so equal scores always resolve the same way.
        scored.sort_by(|(a, _), (b, _)| b.total_cmp(a));
        let (top, account) = scored.first()?;
        // Softmax over the log scores: the share of the posterior mass which
        // falls on the winner. Subtracting the maximum keeps exp() in range.
        let total = scored.iter().map(|(s, _)| (s - top).exp()).sum::<f64>();
        Some(Candidate {
            account: (*account).clone(),
            confidence: 1.0 / total,
        })
    }

    /// Whether a token appears in so many observations that it cannot tell
    /// candidates apart. Imported descriptions repeat field separators, card
    /// numbers and words like "Belastung" on nearly every line; counting them
    /// as independent evidence is what drives the scores to saturate.
    fn is_ubiquitous(&self, token: &str) -> bool {
        let df = self.count_by_token.get(token).copied().unwrap_or_default();
        df >= MIN_OCCURRENCES_TO_PRUNE && df as f64 > MAX_DOCUMENT_FREQUENCY * self.count as f64
    }

    fn score(&self, candidate: &str, tokens: &HashSet<String>) -> f64 {
        let count = self.count_by_account[candidate] as f64;
        let total = self.count as f64;
        let mut score = (count / total).ln();
        for token in tokens {
            // An unseen token is treated as if it had been observed once
            // across the whole corpus. The penalty deliberately does not
            // depend on the candidate: making it depend on the candidate's own
            // count (as Lidstone smoothing would) charges a rare account far
            // less for an unseen token than a common one, which measured far
            // worse than this on a real journal.
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

pub fn transactions(tree: &SyntaxTree) -> impl Iterator<Item = &Transaction> {
    tree.directives.iter().filter_map(|d| match d {
        Directive::Transaction(t) => Some(t),
        _ => None,
    })
}

/// The tokens describing a booking: the words of the description, plus the
/// commodity, the quantity and the account on the other side.
///
/// Description words are stripped of surrounding punctuation, so that
/// `"WIEDIKON,"` matches `"Wiedikon"` and the `/` separating the fields of an
/// imported description drops out entirely. The other three are structured
/// values whose punctuation is meaningful (`Assets:Bank`, `19.10`), so they
/// are only lowercased.
fn tokenize(source: &str, t: &Transaction, b: &Booking, other: &str) -> HashSet<String> {
    source[t.description.content.clone()]
        .split_whitespace()
        .map(|word| word.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|word| !word.is_empty())
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
        let mut model = Model::new("Expenses:TBD");
        model.train(TRAINING, &parse_text(TRAINING).unwrap());
        model
    }

    /// Runs inference over `source` and returns the rewritten text.
    fn infer(source: &str) -> String {
        let model = model();
        apply(
            source,
            model.infer(source, &parse_text(source).unwrap(), 0.0),
        )
    }

    #[test]
    fn test_infer() {
        let source = "2024-02-01 \"Migros Zuerich\"\nAssets:Bank Expenses:TBD 12.00 CHF\n";
        assert_eq!(
            infer(source),
            "2024-02-01 \"Migros Zuerich\"\nAssets:Bank Expenses:Groceries 12.00 CHF\n"
        );
    }

    /// The description decides between accounts seen with the same counterpart.
    #[test]
    fn test_infer_uses_description() {
        let source = "2024-02-01 \"SBB Ticket\"\nAssets:Bank Expenses:TBD 12.00 CHF\n";
        assert_eq!(
            infer(source),
            "2024-02-01 \"SBB Ticket\"\nAssets:Bank Expenses:Travel 12.00 CHF\n"
        );
    }

    /// The placeholder is replaced on whichever side it appears.
    #[test]
    fn test_infer_credit_side() {
        let source = "2024-02-01 \"Salary\"\nExpenses:TBD Assets:Bank 5000.00 CHF\n";
        assert_eq!(
            infer(source),
            "2024-02-01 \"Salary\"\nIncome:Salary Assets:Bank 5000.00 CHF\n"
        );
    }

    /// The account on the other side of the booking is never its own counterpart.
    #[test]
    fn test_infer_excludes_other_account() {
        let source = "2024-02-01 \"Migros Zuerich\"\nExpenses:Groceries Expenses:TBD 12.00 CHF\n";
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
                      Assets:Bank Expenses:TBD 12.00 CHF\n\
                      Assets:Bank Expenses:TBD 13.00 CHF\n";
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
        let source = "2024-02-01 \"Migros\"\nAssets:Bank Expenses:TBD 12.00 CHF\n";
        let model = Model::new("Expenses:TBD");
        assert_eq!(
            model.infer(source, &parse_text(source).unwrap(), 0.0),
            vec![]
        );
    }

    /// Placeholder bookings in the training file carry no information.
    #[test]
    fn test_train_skips_placeholder() {
        let source = "2024-01-01 \"Migros\"\nAssets:Bank Expenses:TBD 50.00 CHF\n";
        let mut model = Model::new("Expenses:TBD");
        model.train(source, &parse_text(source).unwrap());
        assert_eq!(model.count, 0);
        assert!(model.count_by_account.is_empty());
    }

    /// A transaction sharing no tokens with the training data leaves all
    /// candidates tied on their token contributions, so only the prior
    /// separates them and the confidence stays low.
    #[test]
    fn test_confidence_is_low_without_evidence() {
        let source = "2024-02-01 \"Zahnarzt Winterthur\"\nAssets:Bank Expenses:TBD 240.00 CHF\n";
        let tree = parse_text(source).unwrap();
        let t = transactions(&tree).next().unwrap();
        let c = model()
            .predict(source, t, &t.bookings[0], "Assets:Bank")
            .unwrap();
        assert!(c.confidence < 0.9, "{c:?}");
    }

    /// A description the model has seen is decided with high confidence.
    #[test]
    fn test_confidence_is_high_with_evidence() {
        let source = "2024-02-01 \"Migros Zuerich\"\nAssets:Bank Expenses:TBD 12.00 CHF\n";
        let tree = parse_text(source).unwrap();
        let t = transactions(&tree).next().unwrap();
        let c = model()
            .predict(source, t, &t.bookings[0], "Assets:Bank")
            .unwrap();
        assert_eq!(c.account, "Expenses:Groceries");
        assert!(c.confidence > 0.9, "{c:?}");
    }

    /// Below the threshold the placeholder survives untouched.
    #[test]
    fn test_infer_abstains_below_threshold() {
        let source = "2024-02-01 \"Zahnarzt Winterthur\"\nAssets:Bank Expenses:TBD 240.00 CHF\n";
        let tree = parse_text(source).unwrap();
        assert_eq!(model().infer(source, &tree, 0.9), vec![]);
        assert!(!model().infer(source, &tree, 0.0).is_empty());
    }

    /// A threshold above 1 rejects everything, however clear the match.
    #[test]
    fn test_infer_abstains_always_above_one() {
        let source = "2024-02-01 \"Migros Zuerich\"\nAssets:Bank Expenses:TBD 12.00 CHF\n";
        let tree = parse_text(source).unwrap();
        assert_eq!(model().infer(source, &tree, 1.1), vec![]);
    }

    /// Description words lose their punctuation, so that the field separators
    /// and trailing commas of an imported description stop being evidence.
    /// Commodity, quantity and the other account keep theirs.
    #[test]
    fn test_tokenize_normalizes_description_words() {
        let source = "2024-01-01 \"MIGROS WIEDIKON, ZUERICH / Migros\"\n\
                      Assets:Bank Expenses:Groceries 50.00 CHF\n";
        let tree = parse_text(source).unwrap();
        let t = transactions(&tree).next().unwrap();
        let tokens = tokenize(source, t, &t.bookings[0], "Assets:Bank");
        assert!(tokens.contains("wiedikon"), "{tokens:?}");
        assert!(!tokens.contains("wiedikon,"), "{tokens:?}");
        assert!(!tokens.contains("/"), "{tokens:?}");
        // "MIGROS" and "Migros" collapse to one token.
        assert_eq!(tokens.iter().filter(|s| *s == "migros").count(), 1);
        assert!(tokens.contains("assets:bank"), "{tokens:?}");
        assert!(tokens.contains("50.00"), "{tokens:?}");
    }

    /// A token carried by every transaction says nothing about the account, so
    /// once it is common enough to judge it stops being scored.
    #[test]
    fn test_ubiquitous_tokens_are_ignored() {
        let mut source = String::new();
        for i in 0..30 {
            let account = match i % 2 {
                0 => "Expenses:Groceries",
                _ => "Expenses:Travel",
            };
            source.push_str(&format!(
                "2024-01-01 \"noise item{i}\"\nAssets:Bank {account} {i}.00 CHF\n\n"
            ));
        }
        let mut model = Model::new("Expenses:TBD");
        model.train(&source, &parse_text(&source).unwrap());
        assert!(model.is_ubiquitous("noise"));
        // Seen once, so its frequency means nothing yet.
        assert!(!model.is_ubiquitous("item1"));
    }

    /// Small journals keep every token: a frequency is only meaningful once
    /// the token has been seen a fair number of times.
    #[test]
    fn test_small_journals_are_not_stripped() {
        let model = model();
        assert!(!model.is_ubiquitous("assets:bank"));
        assert!(!model.is_ubiquitous("migros"));
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
