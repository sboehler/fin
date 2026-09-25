use std::{error::Error, fs, io::Write, path::PathBuf};

use clap::Args;

use super::crossvalidate::{Item, cross_validate, items};
use crate::syntax::{
    bayes::{Model, apply, is_income_or_expenses, transactions},
    format::format_file,
    parse_file, parse_files, parse_text,
};

/// The placeholder the importers book counter-postings to.
const TBD_ACCOUNT: &str = "Expenses:TBD";

/// Coverage levels reported by `--evaluate`. Indexing the table by coverage
/// rather than by confidence keeps two runs comparable: a change to the
/// features moves the whole confidence scale, so the same threshold means
/// something different before and after, while the same coverage does not.
const COVERAGES: &[f64] = &[1.0, 0.9, 0.8, 0.7, 0.6, 0.5, 0.4, 0.3];

#[derive(Args)]
pub struct Command {
    /// The journal whose placeholder accounts should be replaced.
    #[arg(required_unless_present = "evaluate")]
    target: Option<PathBuf>,

    /// The journal to learn from. Its includes are followed. May be the
    /// target file itself.
    #[arg(short, long)]
    training_file: PathBuf,

    /// The placeholder account to replace.
    #[arg(short, long, default_value = TBD_ACCOUNT)]
    account: String,

    /// Leave the placeholder in place unless the best candidate reaches this
    /// share of the posterior. Run `--evaluate` to pick a value.
    #[arg(short, long, default_value_t = 0.9)]
    min_confidence: f64,

    /// Write the result back to the target file instead of to stdout.
    #[arg(short, long)]
    inplace: bool,

    /// Instead of inferring, cross-validate on the training file and report
    /// how accurate the model is at each confidence threshold.
    #[arg(short, long)]
    evaluate: bool,

    /// Number of cross-validation folds used by `--evaluate`.
    #[arg(long, default_value_t = 5)]
    folds: usize,
}

impl Command {
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        if self.evaluate {
            return self.evaluate();
        }
        let mut model = Model::new(&self.account);
        for (tree, file) in parse_files(&self.training_file)? {
            model.train(&file.text, &tree);
        }

        let target = self.target.as_ref().expect("target is required");
        let (tree, file) = parse_file(target)?;
        let edits = model.infer(&file.text, &tree, self.min_confidence);
        let replaced = edits.len();
        let inferred = apply(&file.text, edits);

        // The replacements change the width of the account column, so the
        // result is formatted rather than written out verbatim.
        let mut out = Vec::new();
        format_file(&mut out, &inferred, &parse_text(&inferred)?)?;

        if self.inplace {
            fs::write(target, &out)?;
        } else {
            std::io::stdout().lock().write_all(&out)?;
        }
        let remaining = count_placeholders(&inferred, &self.account)?;
        eprintln!(
            "replaced {replaced} placeholder(s), {remaining} left below the \
             confidence threshold of {:.3}",
            self.min_confidence
        );
        Ok(())
    }

    /// Splits the training transactions into folds, and for each fold trains on
    /// the others and predicts the accounts of the held-out bookings. Both
    /// sides of every booking are predicted from the other, as the model is
    /// symmetric, but the two directions are reported apart: only one of them
    /// is the job a placeholder actually asks it to do.
    fn evaluate(&self) -> Result<(), Box<dyn Error>> {
        let files = parse_files(&self.training_file)?;
        let items = items(&files);
        if items.is_empty() {
            return Err("no transactions to evaluate".into());
        }
        let mut results = Vec::new();
        let folds = cross_validate(&items, &self.account, self.folds, |model, item| {
            results.extend(self.predictions(model, item));
        });
        if results.is_empty() {
            return Err("no assigned bookings to evaluate".into());
        }

        println!(
            "{} transactions, {} held-out predictions over {folds} folds",
            items.len(),
            results.len()
        );
        // The placeholder stands in for a category account, so that is the
        // number which describes what inference actually does. Predicting the
        // bank or card account instead is a different, harder task and is
        // reported separately rather than averaged in.
        report(
            "predicting an income/expenses account (what infer does)",
            &results
                .iter()
                .filter(|p| p.target_is_ie)
                .collect::<Vec<_>>(),
        );
        report(
            "predicting an assets/liabilities/equity account",
            &results
                .iter()
                .filter(|p| !p.target_is_ie)
                .collect::<Vec<_>>(),
        );
        Ok(())
    }

    /// Predicts each side of every assigned booking from the other.
    fn predictions(&self, model: &Model, item: &Item) -> Vec<Prediction> {
        let (source, t) = (item.source(), item.transaction);
        let mut results = Vec::new();
        for b in t.bookings.iter() {
            let credit = &source[b.credit.range.clone()];
            let debit = &source[b.debit.range.clone()];
            if credit == self.account || debit == self.account {
                continue;
            }
            for (truth, other) in [(credit, debit), (debit, credit)] {
                if let Some(c) = model.predict(source, t, b, other) {
                    results.push(Prediction {
                        confidence: c.confidence,
                        correct: c.account == truth,
                        target_is_ie: is_income_or_expenses(truth),
                    });
                }
            }
        }
        results
    }
}

/// One held-out prediction made during cross-validation.
struct Prediction {
    confidence: f64,
    correct: bool,
    /// Whether the account being predicted is an income or expenses account,
    /// which is the side a placeholder normally stands in for.
    target_is_ie: bool,
}

/// Prints the accuracy / coverage tradeoff for one group of predictions.
fn report(title: &str, results: &[&Prediction]) {
    println!("\n{title}: {} predictions", results.len());
    if results.is_empty() {
        return;
    }
    let mut sorted = results.to_vec();
    sorted.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
    println!(
        "{:>9}  {:>10}  {:>9}  {:>16}",
        "coverage", "threshold", "accuracy", "correct/answered"
    );
    for &target in COVERAGES {
        let n = ((target * sorted.len() as f64).round() as usize).clamp(1, sorted.len());
        let correct = sorted[..n].iter().filter(|p| p.correct).count();
        // The confidence of the last prediction accepted is the threshold
        // which produces this coverage.
        let threshold = sorted[n - 1].confidence;
        let coverage = 100.0 * n as f64 / sorted.len() as f64;
        let accuracy = 100.0 * correct as f64 / n as f64;
        let ratio = format!("{correct}/{n}");
        println!("{coverage:>8.1}%  {threshold:>10.6}  {accuracy:>8.1}%  {ratio:>16}");
    }
}

/// Counts the placeholders left in the result, so the user knows how much is
/// still waiting to be assigned by hand.
fn count_placeholders(source: &str, account: &str) -> Result<usize, Box<dyn Error>> {
    let tree = parse_text(source)?;
    Ok(transactions(&tree)
        .flat_map(|t| t.bookings.iter())
        .flat_map(|b| [b.credit, b.debit])
        .filter(|a| source[a.range.clone()] == *account)
        .count())
}
