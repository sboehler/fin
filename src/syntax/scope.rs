use std::ops::Range;

use super::cst::Token;
use super::error::SyntaxError;
use super::scanner::Scanner;

/// A production in progress.
///
/// Opening a scope notes on the scanner's stack what is being parsed and from
/// where; dropping it takes the note back. A read which fails names every
/// production on the stack, so a failure deep in a file is reported as the
/// chain that wanted it — a digit, in a price, in a directive — without a
/// call site having to say so. The scope itself is only needed to bound the
/// production and to give its node a range.
#[must_use = "a scope which is not bound ends at once, and its production is \
              missing from every error below it"]
pub struct Scope<'s, 'a> {
    scanner: &'s Scanner<'a>,
    start: usize,
    depth: usize,
}

impl<'s, 'a> Scope<'s, 'a> {
    /// Scopes come from [`Scanner::enter`], which is what pushes the
    /// production this one is to unwind: `depth` is the depth it was pushed
    /// at, and nothing else would do.
    pub(super) fn new(scanner: &'s Scanner<'a>, start: usize, depth: usize) -> Self {
        Scope {
            scanner,
            start,
            depth,
        }
    }

    /// The same span under another name, for when what looked like a
    /// directive turns out to be a price. The scope it returns has to be
    /// bound: what is parsed under a name is what is parsed before it drops.
    pub fn with(&self, token: Token) -> Scope<'s, 'a> {
        self.scanner.enter_at(token, self.start)
    }

    /// The text read since the production began.
    pub fn range(&self) -> Range<usize> {
        self.start..self.scanner.pos()
    }

    /// The scanner this production is reading from.
    pub fn scanner(&self) -> &'s Scanner<'a> {
        self.scanner
    }

    /// The whole text being parsed.
    pub fn source(&self) -> &'a str {
        self.scanner.source
    }

    /// This production failed, wanting `want`. What it is itself does not
    /// appear in the error: `want` is the better word for the same span.
    pub fn error(&self, want: Token) -> SyntaxError {
        self.scanner.error(self.depth, self.start, want)
    }

    /// This production failed, with nothing more to say than its own name.
    pub fn token_error(&self) -> SyntaxError {
        self.error(self.token())
    }

    fn token(&self) -> Token {
        self.scanner.token(self.depth)
    }
}

impl Drop for Scope<'_, '_> {
    fn drop(&mut self) {
        self.scanner.unwind(self.depth);
    }
}
