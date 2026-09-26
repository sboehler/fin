use std::{cmp::Reverse, ops::Range};

use super::bayes::apply;
use super::cst::{Addon, Booking, Bookings, Directive, SyntaxTree, Transaction};
use crate::model::entities::AccountType;

/// The width a description is wrapped to, indentation included.
pub const DEFAULT_WIDTH: usize = 80;

/// Rewrites the transactions written one booking per line into the arrow
/// notation. Everything else in the file, transactions already written as
/// groups included, is left as it is.
///
/// The result is parseable but not aligned: run it through
/// [`super::format::format_file`] to lay out the columns.
pub fn migrate(source: &str, tree: &SyntaxTree, width: usize) -> String {
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    for d in &tree.directives {
        if let Directive::Transaction(t) = d
            && let Bookings::Lines(bookings) = &t.bookings
            && let Some(text) = transaction(source, t, bookings, width)
        {
            // The range of a transaction takes in the newline ending its last
            // line, which the replacement has to put back.
            let newline = source[t.range.clone()].ends_with('\n');
            let text = text + if newline { "\n" } else { "" };
            edits.push((t.range.clone(), text));
        }
    }
    apply(source, edits)
}

/// One booking of the old notation, with its sign normalized: a negative
/// quantity is the same booking the other way round, and only then does the
/// direction of an arrow say where the money went.
#[derive(Clone, Copy)]
struct Flow<'a> {
    credit: &'a str,
    debit: &'a str,
    quantity: &'a str,
    commodity: &'a str,
}

impl<'a> Flow<'a> {
    fn new(source: &'a str, b: &Booking) -> Self {
        let (credit, debit) = (
            &source[b.credit.range.clone()],
            &source[b.debit.range.clone()],
        );
        let quantity = &source[b.quantity.0.clone()];
        let (credit, debit, quantity) = match quantity.strip_prefix('-') {
            Some(positive) => (debit, credit, positive),
            None => (credit, debit, quantity),
        };
        Flow {
            credit,
            debit,
            quantity,
            commodity: &source[b.commodity.0.clone()],
        }
    }

    /// The account on the other side of `account`, and whether `account` is
    /// the credit, which is the way the arrow will point.
    fn other(&self, account: &str) -> Option<(&'a str, bool)> {
        if self.credit == account {
            Some((self.debit, true))
        } else if self.debit == account {
            Some((self.credit, false))
        } else {
            None
        }
    }
}

/// The transaction in the arrow notation, or `None` if it cannot be written
/// in it: a description is mandatory there, and an empty one would be lost.
fn transaction(
    source: &str,
    t: &Transaction,
    bookings: &[Booking],
    width: usize,
) -> Option<String> {
    let description = t.description.text(source);
    if description.trim().is_empty() || bookings.is_empty() {
        return None;
    }
    let mut lines = Vec::new();
    if let Some(Addon::Performance { range, .. } | Addon::Accrual { range, .. }) = &t.addon {
        lines.push(source[range.clone()].to_string());
    }
    lines.push(source[t.date.0.clone()].to_string());
    lines.extend(
        wrap(&description, width.saturating_sub(INDENT.len())).map(|l| INDENT.to_string() + l),
    );
    let flows = bookings.iter().map(|b| Flow::new(source, b)).collect();
    for group in groups(flows) {
        lines.extend(group);
    }
    Some(lines.join("\n"))
}

/// What a description line is indented by.
const INDENT: &str = "  ";

/// The lines of every group of the transaction, the largest group first.
fn groups(mut flows: Vec<Flow>) -> Vec<Vec<String>> {
    let mut groups = Vec::new();
    while !flows.is_empty() {
        let account = hub(&flows);
        let (mine, rest): (Vec<_>, Vec<_>) = flows
            .iter()
            .copied()
            .partition(|f| f.other(account).is_some());
        groups.push((mine.len(), group(account, &mine)));
        flows = rest;
    }
    // The hub of a group is the account of most of the remaining bookings, so
    // the groups come out largest first already; sorting says so out loud.
    groups.sort_by(|(a, _), (b, _)| b.cmp(a));
    groups.into_iter().map(|(_, lines)| lines).collect()
}

/// The account the group is written around: the one appearing in the most
/// bookings. Of two accounts appearing equally often, the one money sits in
/// is the one the reader is looking for, and of two of those, the one written
/// first.
fn hub<'a>(flows: &[Flow<'a>]) -> &'a str {
    let accounts = flows.iter().flat_map(|f| [f.credit, f.debit]);
    accounts
        .clone()
        .max_by_key(|a| {
            (
                flows.iter().filter(|f| f.other(a).is_some()).count(),
                is_al(a),
                // Earliest first, so that the credit of a lone booking
                // between two accounts of the same kind leads it.
                Reverse(accounts.clone().position(|b| b == *a)),
            )
        })
        .expect("a group has at least one booking")
}

fn is_al(account: &str) -> bool {
    AccountType::try_from(account.split(':').next().unwrap_or_default())
        .is_ok_and(|account_type| account_type.is_al())
}

/// The hub at column zero, then the accounts facing it: those it receives
/// from first, then those it pays.
fn group(account: &str, flows: &[Flow]) -> Vec<String> {
    let mut lines = vec![account.to_string()];
    let legs = flows
        .iter()
        .filter_map(|f| f.other(account).map(|o| (o, f)));
    for (arrow, hub_credits) in [("<-", false), ("->", true)] {
        for ((other, _), f) in legs.clone().filter(|((_, c), _)| *c == hub_credits) {
            lines.push(format!(
                "{arrow} {other} {quantity} {commodity}",
                quantity = f.quantity,
                commodity = f.commodity
            ));
        }
    }
    lines
}

/// The text broken into lines of at most `width` characters, at the spaces
/// between words. A word longer than `width` gets a line of its own rather
/// than being cut in two.
fn wrap(text: &str, width: usize) -> impl Iterator<Item = &str> {
    let mut rest = text.trim();
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let end = match rest.char_indices().map(|(i, _)| i).nth(width) {
            // The rest fits on one line.
            None => rest.len(),
            // Break at the last space which fits, or at the first one beyond
            // the width if there is none.
            Some(limit) => rest[..limit]
                .rfind(char::is_whitespace)
                .or_else(|| rest[limit..].find(char::is_whitespace).map(|i| i + limit))
                .unwrap_or(rest.len()),
        };
        let (line, tail) = rest.split_at(end);
        rest = tail.trim_start();
        Some(line.trim_end())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::parse_text;
    use pretty_assertions::assert_eq;

    /// The journal in the arrow notation. The result is parsed as well, since
    /// a migration which does not is no use.
    fn migrated(source: &str) -> String {
        let tree = parse_text(source).expect("parses");
        let result = migrate(source, &tree, DEFAULT_WIDTH);
        parse_text(&result).expect("migrates to a journal which parses");
        result
    }

    /// The bookings of the journal, with their signs normalized, as
    /// `credit debit quantity commodity`.
    fn bookings(source: &str) -> Vec<String> {
        let tree = parse_text(source).expect("parses");
        let mut res: Vec<String> = tree
            .directives
            .iter()
            .filter_map(|d| match d {
                Directive::Transaction(t) => Some(&t.bookings),
                _ => None,
            })
            .flat_map(Bookings::iter)
            .map(|b| {
                let (credit, debit) = (
                    &source[b.credit.range.clone()],
                    &source[b.debit.range.clone()],
                );
                let quantity = &source[b.quantity.0.clone()];
                let commodity = &source[b.commodity.0.clone()];
                match quantity.strip_prefix('-') {
                    Some(q) => format!("{debit} {credit} {q} {commodity}"),
                    None => format!("{credit} {debit} {quantity} {commodity}"),
                }
            })
            .collect();
        res.sort();
        res
    }

    /// A booking between an account money sits in and a category leads with
    /// the former, whichever side of the booking it was written on.
    #[test]
    fn the_asset_account_leads() {
        assert_eq!(
            "2026-06-24\n  Groceries\nAssets:Bank\n-> Expenses:Food 42.50 CHF\n",
            migrated("2026-06-24 \"Groceries\"\nAssets:Bank Expenses:Food 42.50 CHF\n")
        );
        assert_eq!(
            "2026-06-24\n  Salary\nAssets:Bank\n<- Income:Salary 5000 CHF\n",
            migrated("2026-06-24 \"Salary\"\nIncome:Salary Assets:Bank 5000 CHF\n")
        );
    }

    /// Between two accounts of the same kind there is nothing to choose, so
    /// the credit leads and the arrow points forward.
    #[test]
    fn two_accounts_of_a_kind_point_forward() {
        assert_eq!(
            "2026-06-24\n  Transfer\nAssets:Bank\n-> Assets:Savings 100 CHF\n",
            migrated("2026-06-24 \"Transfer\"\nAssets:Bank Assets:Savings 100 CHF\n")
        );
    }

    /// A negative quantity is the same booking the other way round, and only
    /// written that way does the arrow say where the money went.
    #[test]
    fn negative_quantities_turn_the_booking_round() {
        assert_eq!(
            "2026-06-24\n  Refund\nAssets:Bank\n<- Expenses:Food 42.50 CHF\n",
            migrated("2026-06-24 \"Refund\"\nAssets:Bank Expenses:Food -42.50 CHF\n")
        );
    }

    /// The account of most of the bookings leads the group, and the accounts
    /// it receives from are written before those it pays.
    #[test]
    fn the_account_of_most_bookings_leads() {
        assert_eq!(
            "\
2026-06-24
  Payday
Assets:Bank
<- Income:Salary 5000 CHF
<- Income:Bonus 500 CHF
-> Expenses:Rent 1200 CHF
",
            migrated(
                "\
2026-06-24 \"Payday\"
Income:Salary Assets:Bank 5000 CHF
Assets:Bank Expenses:Rent 1200 CHF
Income:Bonus Assets:Bank 500 CHF
"
            )
        );
    }

    /// The bookings which have no account in common with the first group are
    /// grouped in turn, and the larger group is written first.
    #[test]
    fn groups_come_largest_first() {
        assert_eq!(
            "\
2026-06-24
  Two households
Assets:Bank
-> Expenses:Rent 1200 CHF
-> Expenses:Food 300 CHF
Assets:Card
-> Expenses:Fuel 60 CHF
",
            migrated(
                "\
2026-06-24 \"Two households\"
Assets:Card Expenses:Fuel 60 CHF
Assets:Bank Expenses:Rent 1200 CHF
Assets:Bank Expenses:Food 300 CHF
"
            )
        );
    }

    /// Every booking of an account goes into its group, whichever way it
    /// points: a purchase paid for and delivered through the same broker is
    /// one group, not two.
    #[test]
    fn a_group_holds_both_directions() {
        assert_eq!(
            "\
2026-06-24
  Buy 11 VT
Assets:IBKR
<- Expenses:Trading 11 VT
-> Expenses:Trading 1698.95 USD
-> Expenses:Fees 1.00 USD
",
            migrated(
                "\
2026-06-24 \"Buy 11 VT\"
Expenses:Trading Assets:IBKR 11 VT
Assets:IBKR Expenses:Trading 1698.95 USD
Assets:IBKR Expenses:Fees 1.00 USD
"
            )
        );
    }

    /// The arrow notation writes the description unquoted below the date,
    /// where it is wrapped at the width rather than running off the page.
    #[test]
    fn descriptions_are_wrapped() {
        let source = "2026-06-24 \"A description long enough that it has to be \
                      wrapped over two lines, and then some\"\n\
                      Assets:Bank Expenses:Food 1 CHF\n";
        assert_eq!(
            "2026-06-24\n\
             \x20 A description long enough that it has to be wrapped over two lines, and then\n\
             \x20 some\n\
             Assets:Bank\n\
             -> Expenses:Food 1 CHF\n",
            migrated(source)
        );
    }

    /// A word longer than the width keeps a line of its own rather than being
    /// cut in two.
    #[test]
    fn long_words_are_left_whole() {
        assert_eq!(
            vec!["a", "0123456789012345", "b"],
            wrap("a 0123456789012345 b", 10).collect::<Vec<_>>()
        );
    }

    /// Transactions already in the arrow notation, and everything which is
    /// not a transaction, are copied over as they are.
    #[test]
    fn leaves_the_rest_of_the_file_alone() {
        let source = "\
# a comment
include \"other.journal\"
2026-06-23 open Assets:Bank
2026-06-24 price VT 154.45 USD

2026-06-25
  Already migrated
Assets:Bank
-> Expenses:Food 1 CHF

2026-06-26 balance Assets:Bank 100 CHF
";
        assert_eq!(source, migrated(source));
    }

    /// A transaction the arrow notation cannot hold is left in the old one,
    /// rather than losing what it says.
    #[test]
    fn keeps_transactions_without_a_description() {
        let source = "2026-06-24 \"\"\nAssets:Bank Expenses:Food 1 CHF\n";
        assert_eq!(source, migrated(source));
    }

    /// Migrating rewrites how the bookings are written, not what they say.
    #[test]
    fn keeps_the_bookings() {
        let source = "\
@performance(VT,USD)
2026-06-24 \"Buy 11 VT\"
Expenses:Trading Assets:IBKR 11 VT
Assets:IBKR Expenses:Trading 1698.95 USD
Assets:IBKR Expenses:Fees 1.00 USD

2026-06-25 \"Payday\"
Income:Salary Assets:Bank 5000 CHF
Assets:Bank Expenses:Rent -1200 CHF
";
        let result = migrated(source);
        assert_eq!(bookings(source), bookings(&result));
        assert!(result.starts_with("@performance(VT,USD)\n"));
    }
}
