//! Finds bookings whose account disagrees with what the rest of the journal
//! suggests.

use std::{error::Error, path::PathBuf};

use clap::Args;

use super::crossvalidate::{Item, cross_validate, items};
use crate::syntax::{
    bayes::{Model, is_income_or_expenses},
    parse_files,
};

/// The placeholder the importers book counter-postings to. Bookings which
/// still carry it are unclassified rather than misclassified, so they are not
/// reviewed.
const TBD_ACCOUNT: &str = "Expenses:TBD";

#[derive(Args)]
pub struct Command {
    /// The journal to review. Its includes are followed.
    journal: PathBuf,

    /// Only report a disagreement this confident. The model is right about
    /// 93% of the time at 0.98 and 96% at 0.9998 on a real journal, so a
    /// lower value finds more mistakes but wastes more of your time.
    #[arg(short, long, default_value_t = 0.99)]
    min_confidence: f64,

    /// The placeholder account, which is skipped.
    #[arg(short, long, default_value = TBD_ACCOUNT)]
    account: String,

    /// Number of cross-validation folds. More folds train each model on more
    /// of the journal, at proportionally more work.
    #[arg(long, default_value_t = 5)]
    folds: usize,

    /// Report at most this many findings, most confident first.
    #[arg(short, long, default_value_t = 50)]
    limit: usize,

    /// Only question income and expenses accounts. Transfers between own
    /// accounts are reported otherwise, and the model has no way of telling
    /// whose card a payment settles.
    #[arg(long)]
    only_income_expenses: bool,
}

impl Command {
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        let files = parse_files(&self.journal)?;
        let items = items(&files);
        if items.is_empty() {
            return Err("no transactions to review".into());
        }

        let (findings, reviewed) = self.collect(&items);
        for finding in findings.iter().take(self.limit) {
            println!(
                "{location}  {confidence:.4}\n  {date} \"{description}\"\n  {current} -> {suggested}\n",
                location = finding.location,
                confidence = finding.confidence,
                date = finding.date,
                description = finding.description,
                current = finding.current,
                suggested = finding.suggested,
            );
        }
        eprintln!(
            "{} of {} bookings reviewed disagree at {:.4} or above{}",
            findings.len(),
            reviewed,
            self.min_confidence,
            match findings.len() > self.limit {
                true => format!(", showing the {} most confident", self.limit),
                false => String::new(),
            }
        );
        Ok(())
    }

    /// Cross-validates over `items` and returns the disagreements which reach
    /// the confidence threshold, most confident first, along with the number
    /// of bookings looked at.
    fn collect(&self, items: &[Item]) -> (Vec<Finding>, usize) {
        let mut findings = Vec::new();
        let mut reviewed = 0;
        cross_validate(items, &self.account, self.folds, |model, item| {
            reviewed += self.review(model, item, &mut findings);
        });
        findings.retain(|f| f.confidence >= self.min_confidence);
        findings.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
        (findings, reviewed)
    }

    /// Both sides of every booking are re-predicted from the other. A
    /// disagreement is a candidate mistake; whether it is really one is for
    /// the reader to say, since the model is wrong a few percent of the time
    /// even when confident.
    fn review(&self, model: &Model, item: &Item, findings: &mut Vec<Finding>) -> usize {
        let source = item.source();
        let t = item.transaction;
        let mut reviewed = 0;
        for b in t.bookings.iter() {
            let credit = &source[b.credit.range.clone()];
            let debit = &source[b.debit.range.clone()];
            if credit == self.account || debit == self.account {
                continue;
            }
            reviewed += 1;
            // Both sides are questioned, but a booking is one thing to look
            // at, so only the stronger of the two disagreements is reported.
            let mut best: Option<Finding> = None;
            for (current, other) in [(credit, debit), (debit, credit)] {
                if self.only_income_expenses && !is_income_or_expenses(current) {
                    continue;
                }
                let Some(c) = model.predict(source, t, b, other) else {
                    continue;
                };
                if c.account == current {
                    continue;
                }
                let finding = Finding {
                    location: item.location(b.range.start),
                    confidence: c.confidence,
                    date: source[t.date.0.clone()].to_string(),
                    description: t.description.text(source).into_owned(),
                    current: current.to_string(),
                    suggested: c.account,
                };
                if best
                    .as_ref()
                    .is_none_or(|b| finding.confidence > b.confidence)
                {
                    best = Some(finding);
                }
            }
            findings.extend(best);
        }
        reviewed
    }
}

#[derive(Debug, PartialEq)]
struct Finding {
    location: String,
    confidence: f64,
    date: String,
    description: String,
    current: String,
    suggested: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::{parse_text, sourcefile::SourceFile};
    use pretty_assertions::assert_eq;

    /// A journal where "Coop" is groceries nine times out of ten, and once --
    /// the transaction on 2024-02-01 -- is booked to travel instead.
    fn journal() -> String {
        let mut journal = String::new();
        for day in 1..=9 {
            journal.push_str(&format!(
                "2024-01-0{day} \"Coop Zuerich\"\nAssets:Bank Expenses:Groceries 50.00 CHF\n\n"
            ));
        }
        journal.push_str("2024-02-01 \"Coop Zuerich\"\nAssets:Bank Expenses:Travel 50.00 CHF\n");
        journal
    }

    fn command() -> Command {
        Command {
            journal: PathBuf::new(),
            min_confidence: 0.9,
            account: TBD_ACCOUNT.to_string(),
            folds: 5,
            limit: 50,
            only_income_expenses: false,
        }
    }

    fn findings(command: &Command, source: &str) -> Vec<Finding> {
        let files = vec![(
            parse_text(source).unwrap(),
            SourceFile {
                path: None,
                text: source.to_string(),
            },
        )];
        command.collect(&items(&files)).0
    }

    /// The odd one out is reported, and nothing else is.
    #[test]
    fn test_finds_the_outlier() {
        let journal = journal();
        let found = findings(&command(), &journal);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].date, "2024-02-01");
        assert_eq!(found[0].current, "Expenses:Travel");
        assert_eq!(found[0].suggested, "Expenses:Groceries");
    }

    /// A booking is one thing to look at, however many of its sides the model
    /// disagrees with.
    #[test]
    fn test_reports_each_booking_once() {
        let found = findings(&command(), &journal());
        let locations = found.iter().map(|f| &f.location).collect::<Vec<_>>();
        let unique = locations.iter().collect::<std::collections::HashSet<_>>();
        assert_eq!(locations.len(), unique.len(), "{locations:?}");
    }

    /// A consistent journal has nothing to report.
    #[test]
    fn test_consistent_journal_is_quiet() {
        let consistent = journal().replace("Expenses:Travel", "Expenses:Groceries");
        assert_eq!(findings(&command(), &consistent), vec![]);
    }

    /// Raising the threshold drops the weaker findings.
    #[test]
    fn test_threshold_filters() {
        let mut command = command();
        command.min_confidence = 1.1;
        assert_eq!(findings(&command, &journal()), vec![]);
    }

    /// Bookings still carrying the placeholder are unclassified, not
    /// misclassified, so they are left out.
    #[test]
    fn test_placeholder_is_not_reviewed() {
        let journal = journal().replace("Expenses:Travel", TBD_ACCOUNT);
        assert_eq!(findings(&command(), &journal), vec![]);
    }
}
