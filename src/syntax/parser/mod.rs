use crate::syntax::cst::{SyntaxTree, Token};
use crate::syntax::error::SyntaxError;
use crate::syntax::scanner::Scanner;

use self::directive::directive;
use self::lexical::comment;

mod directive;
mod lexical;
mod transaction;

pub type Result<T> = std::result::Result<T, SyntaxError>;

/// Parses a whole journal.
pub fn parse(text: &str) -> Result<SyntaxTree> {
    file(&Scanner::new(text))
}

fn file(s: &Scanner) -> Result<SyntaxTree> {
    // A file is not a production in progress: naming it in every error
    // below would say nothing.
    let start = s.pos();
    let mut directives = Vec::new();
    while let Some(c) = s.current() {
        match c {
            '*' | '/' | '#' => {
                comment(s)?;
            }
            c if c.is_ascii_digit() || c == 'i' || c == '@' || c == 'v' => {
                let d = directive(s)?;
                directives.push(d)
            }
            c if c.is_whitespace() => {
                s.read_rest_of_line()?;
            }
            _ => {
                let scope = s.enter(Token::Either(vec![
                    Token::Date,
                    Token::Include,
                    Token::Addon,
                    Token::BlankLine,
                ]));
                s.advance();
                return Err(scope.token_error());
            }
        }
    }
    Ok(SyntaxTree {
        range: start..s.pos(),
        directives,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// Every production takes its frame off the stack again, however it
    /// ends, so a failure names the productions it is in and no others.
    #[test]
    fn the_stack_unwinds() {
        for f in [
            "2024-12-31 open Assets:Foo\n",
            "2024-12-31 opne Assets:Foo\n",
            "2024-12-31 price FOO 1.5 BAR\n\n2024-12-31 close Assets:Foo\n",
            "2024-12-31\n  Buy\n  and more\nAssets:Foo\n-> Assets:Bar 1 CHF\n",
            "2024-12-31\n  Buy\nAssets:Foo 1 CHF\n-> Assets:Bar 2 CHF\n",
            "2024-12-31 \"Buy\"\nAssets:Foo Assets:Bar\n",
            "not a directive\n",
        ] {
            let s = Scanner::new(f);
            let _ = file(&s);
            assert_eq!(0, s.depth(), "{f}");
        }
    }
}
