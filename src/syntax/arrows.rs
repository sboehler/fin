//! Writing bookings in the arrow notation: which account a group is written
//! around, which way its arrows point, and in which order.

use std::cmp::Reverse;

use crate::model::entities::AccountType;

/// The width a description is wrapped to, indentation included.
pub const DEFAULT_WIDTH: usize = 80;

/// One booking: money flowing from `credit` to `debit`.
#[derive(Clone, Copy)]
pub struct Flow<'a> {
    credit: &'a str,
    debit: &'a str,
    quantity: &'a str,
    commodity: &'a str,
}

impl<'a> Flow<'a> {
    /// The sign is normalized: a negative quantity is the same booking the
    /// other way round, and only written that way does the direction of an
    /// arrow say where the money went.
    pub fn new(credit: &'a str, debit: &'a str, quantity: &'a str, commodity: &'a str) -> Self {
        let (credit, debit, quantity) = match quantity.strip_prefix('-') {
            Some(positive) => (debit, credit, positive),
            None => (credit, debit, quantity),
        };
        Flow {
            credit,
            debit,
            quantity,
            commodity,
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

/// The lines of one transaction in the arrow notation, or `None` if it
/// cannot be written in it: a description is mandatory there, and an empty
/// one would be lost.
///
/// The lines are not aligned: run the journal they end up in through
/// [`super::format::format_file`] to lay out the columns.
pub fn transaction(
    addon: Option<&str>,
    date: &str,
    description: &str,
    flows: Vec<Flow>,
    width: usize,
) -> Option<Vec<String>> {
    if description.trim().is_empty() || flows.is_empty() {
        return None;
    }
    let mut lines = Vec::new();
    lines.extend(addon.map(str::to_string));
    lines.push(date.to_string());
    // A description already written over several lines keeps its breaks, and
    // each of its lines is wrapped in turn. A blank line would end the
    // transaction, so there are none.
    for line in description.lines() {
        lines
            .extend(wrap(line, width.saturating_sub(INDENT.len())).map(|l| INDENT.to_string() + l));
    }
    for group in groups(flows) {
        lines.extend(group);
    }
    Some(lines)
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
/// from first, then those it pays, each side by account name.
fn group(account: &str, flows: &[Flow]) -> Vec<String> {
    let mut lines = vec![account.to_string()];
    let legs = flows
        .iter()
        .filter_map(|f| f.other(account).map(|o| (o, f)));
    for (arrow, hub_credits) in [("<-", false), ("->", true)] {
        let mut side: Vec<_> = legs
            .clone()
            .filter(|((_, c), _)| *c == hub_credits)
            .map(|((other, _), f)| (other, f))
            .collect();
        // By account, so that a long group can be read down its accounts.
        // Two bookings of one account keep the order they were written in.
        side.sort_by_key(|&(other, _)| other);
        for (other, f) in side {
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
    use pretty_assertions::assert_eq;

    /// A word longer than the width keeps a line of its own rather than being
    /// cut in two.
    #[test]
    fn long_words_are_left_whole() {
        assert_eq!(
            vec!["a", "0123456789012345", "b"],
            wrap("a 0123456789012345 b", 10).collect::<Vec<_>>()
        );
    }
}
