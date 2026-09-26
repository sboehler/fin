use std::ops::Range;

use super::arrows::{self, Flow};
use super::bayes::apply;
use super::cst::{Addon, Booking, Bookings, Directive, SyntaxTree, Transaction};

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

/// The transaction in the arrow notation, from the text it was written in.
fn transaction(
    source: &str,
    t: &Transaction,
    bookings: &[Booking],
    width: usize,
) -> Option<String> {
    let addon = t.addon.as_ref().map(|a| match a {
        Addon::Performance { range, .. } | Addon::Accrual { range, .. } => &source[range.clone()],
    });
    let flows = bookings.iter().map(|b| flow(source, b)).collect();
    let lines = arrows::transaction(
        addon,
        &source[t.date.0.clone()],
        &t.description.text(source),
        flows,
        width,
    )?;
    Some(lines.join("\n"))
}

/// The booking as a flow between its two accounts.
fn flow<'a>(source: &'a str, b: &Booking) -> Flow<'a> {
    Flow::new(
        &source[b.credit.range.clone()],
        &source[b.debit.range.clone()],
        &source[b.quantity.0.clone()],
        &source[b.commodity.0.clone()],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::arrows::DEFAULT_WIDTH;
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

    /// The account of most of the bookings leads the group, the accounts it
    /// receives from are written before those it pays, and each side is
    /// sorted by account.
    #[test]
    fn the_account_of_most_bookings_leads() {
        assert_eq!(
            "\
2026-06-24
  Payday
Assets:Bank
<- Income:Bonus 500 CHF
<- Income:Salary 5000 CHF
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
-> Expenses:Food 300 CHF
-> Expenses:Rent 1200 CHF
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
-> Expenses:Fees 1.00 USD
-> Expenses:Trading 1698.95 USD
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

    /// Two bookings of one account cannot be told apart by name, so they
    /// keep the order they were written in.
    #[test]
    fn bookings_of_one_account_keep_their_order() {
        assert_eq!(
            "\
2026-06-24
  Shopping
Assets:Bank
-> Expenses:Food 2 CHF
-> Expenses:Food 1 CHF
",
            migrated(
                "\
2026-06-24 \"Shopping\"
Assets:Bank Expenses:Food 2 CHF
Assets:Bank Expenses:Food 1 CHF
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
