use super::cst::{Character, Sequence, Token};
use super::error::SyntaxError;
use super::scope::Scope;
use std::ops::Range;
use std::{cell::RefCell, iter::Peekable, str::CharIndices};

#[derive(Clone)]
pub struct Scanner<'a> {
    pub source: &'a str,
    chars: RefCell<Peekable<CharIndices<'a>>>,
    /// The productions in progress, outermost first. A failed read names
    /// every one of them, so no caller has to say what it was doing.
    stack: RefCell<Vec<Frame>>,
}

/// A production in progress: what it is, and where it began.
#[derive(Clone)]
struct Frame {
    token: Token,
    start: usize,
}

pub type Result<T> = std::result::Result<T, SyntaxError>;

impl<'a> Scanner<'a> {
    pub fn new(text: &'a str) -> Scanner<'a> {
        Scanner {
            source: text,
            chars: RefCell::new(text.char_indices().peekable()),
            stack: RefCell::new(Vec::new()),
        }
    }

    pub fn snapshot(&self) -> Box<dyn FnOnce() + '_> {
        let s = self.chars.borrow().clone();
        Box::new(|| {
            let _ = self.chars.replace(s);
        })
    }

    pub fn range(&self, start: usize) -> Range<usize> {
        start..self.pos()
    }

    /// Begins a production where the scanner stands. It ends when the scope
    /// is dropped.
    pub fn enter(&self, token: Token) -> Scope<'_, 'a> {
        self.enter_at(token, self.pos())
    }

    /// Begins a production over text which began earlier: a price directive
    /// begins at its date, not at the word `price`.
    pub fn enter_at(&self, token: Token, start: usize) -> Scope<'_, 'a> {
        let depth = {
            let mut stack = self.stack.borrow_mut();
            stack.push(Frame { token, start });
            stack.len() - 1
        };
        Scope::new(self, start, depth)
    }

    /// Notes that the production at `depth` has ended, however it ended.
    /// Truncating rather than popping re-establishes the invariant even if
    /// something above was left behind.
    pub fn unwind(&self, depth: usize) {
        self.stack.borrow_mut().truncate(depth);
    }

    pub fn depth(&self) -> usize {
        self.stack.borrow().len()
    }

    /// What the production at `depth` is parsing.
    pub fn token(&self, depth: usize) -> Token {
        self.stack.borrow()[depth].token.clone()
    }

    /// A failure which wanted `want`, from `start` to here, inside the
    /// innermost `depth` productions.
    pub fn error(&self, depth: usize, start: usize, want: Token) -> SyntaxError {
        let mut context = None;
        for frame in self.stack.borrow().iter().take(depth) {
            context = Some(Box::new(SyntaxError {
                range: frame.start..self.pos(),
                want: frame.token.clone(),
                context,
            }));
        }
        SyntaxError {
            range: self.range(start),
            want,
            context,
        }
    }

    /// The read from `start` to here wanted `ch` and did not get it.
    fn character_error(&self, start: usize, ch: &Character) -> SyntaxError {
        self.sequence_error(start, &Sequence::One(ch.clone()))
    }

    /// The read from `start` to here wanted `seq` and did not get it.
    fn sequence_error(&self, start: usize, seq: &Sequence) -> SyntaxError {
        self.error(self.depth(), start, Token::Sequence(seq.clone()))
    }

    pub fn current(&self) -> Option<char> {
        self.chars.borrow_mut().peek().map(|t| t.1)
    }

    pub fn advance(&self) -> Option<char> {
        self.chars.borrow_mut().next().map(|t| t.1)
    }

    pub fn pos(&self) -> usize {
        self.chars
            .borrow_mut()
            .peek()
            .map_or_else(|| self.source.len(), |t| t.0)
    }

    pub fn read_while_1(&self, ch: &Character) -> Result<Range<usize>> {
        let start = self.pos();
        if !ch.is(self.current()) {
            self.advance();
            return Err(self.character_error(start, ch));
        }
        Ok(self.read_while(ch))
    }

    pub fn read_while(&self, ch: &Character) -> Range<usize> {
        let start = self.pos();
        while ch.is(self.current()) {
            self.advance();
        }
        self.range(start)
    }

    pub fn read_until(&self, ch: &Character) -> Range<usize> {
        let start = self.pos();
        while !ch.is(self.current()) {
            self.advance();
        }
        self.range(start)
    }

    pub fn read_char(&self, ch: &Character) -> Result<Range<usize>> {
        let start = self.pos();
        let c = self.advance();
        if ch.is(c) {
            Ok(self.range(start))
        } else {
            Err(self.character_error(start, ch))
        }
    }

    pub fn read_string(&self, str: &str) -> Result<Range<usize>> {
        let start = self.pos();
        for c in str.chars() {
            self.read_char(&Character::Char(c))?;
        }
        Ok(self.range(start))
    }

    pub fn read_sequence(&self, seq: &Sequence) -> Result<Range<usize>> {
        let start = self.pos();
        match seq {
            Sequence::One(ch) => {
                self.read_char(ch)?;
                Ok(self.range(start))
            }
            Sequence::OneOf(seqs) => {
                for s in seqs {
                    let rollback = self.snapshot();
                    if self.read_sequence(s).is_ok() {
                        return Ok(self.range(start));
                    }
                    rollback();
                }
                self.advance();
                Err(self.sequence_error(start, seq))
            }
            Sequence::NumberOf(n, char) => {
                for _ in 0..*n {
                    self.read_char(char)
                        .map_err(|_| self.sequence_error(start, seq))?;
                }
                Ok(self.range(start))
            }
            Sequence::String(s) => {
                for c in s.chars() {
                    self.read_char(&Character::Char(c))
                        .map_err(|_| self.sequence_error(start, seq))?;
                }
                Ok(self.range(start))
            }
        }
    }

    pub fn read_eol(&self) -> Result<Range<usize>> {
        let start = self.pos();
        let c = self.advance();
        match c {
            None | Some('\n') => Ok(self.range(start)),
            _ => Err(self.character_error(
                start,
                &Character::OneOf(vec![Character::NewLine, Character::EOF]),
            )),
        }
    }

    pub fn read_space_1(&self) -> Result<Range<usize>> {
        let start = self.pos();
        match self.current() {
            Some(ch) if !ch.is_ascii_whitespace() => {
                self.advance();
                Err(self.character_error(start, &Character::HorizontalSpace))
            }
            _ => Ok(self.read_space()),
        }
    }

    pub fn read_space(&self) -> Range<usize> {
        self.read_while(&Character::HorizontalSpace)
    }

    pub fn read_rest_of_line(&self) -> Result<Range<usize>> {
        let start = self.pos();
        self.read_while(&Character::HorizontalSpace);
        self.read_eol()?;
        Ok(self.range(start))
    }
}

#[cfg(test)]
mod test_scanner {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_read_while() {
        let text = "aaasdff";
        let s = Scanner::new(text);
        assert_eq!("aaasd", &text[s.read_while(&Character::NotChar('f'))]);
        assert_eq!("ff", &text[s.read_while(&Character::Char('f'))]);
        assert_eq!("", &text[s.read_while(&Character::Char('q'))]);
        assert_eq!(Ok(""), s.read_eol().map(|r| &text[r]));
    }

    #[test]
    fn test_read_while_1() {
        let text = "aaasdff";
        let s = Scanner::new(text);
        assert_eq!(
            Ok("aaasd"),
            s.read_while_1(&Character::NotChar('f')).map(|r| &text[r])
        );
        assert_eq!(
            Ok("ff"),
            s.read_while_1(&Character::Char('f')).map(|r| &text[r])
        );
        assert_eq!(
            Err(SyntaxError {
                range: 7..7,
                want: Token::Sequence(Sequence::One(Character::Char('q'))),
                context: None,
            }),
            s.read_while_1(&Character::Char('q'))
        );
        assert_eq!("", &text[s.read_eol().unwrap()]);
    }

    #[test]
    fn test_read_char() {
        let text = "asdf";
        let s = Scanner::new(text);
        assert_eq!("a", &text[s.read_char(&Character::Char('a')).unwrap()]);
        assert_eq!(
            Err(SyntaxError {
                range: 1..2,
                want: Token::Sequence(Sequence::One(Character::Char('q'))),
                context: None,
            }),
            s.clone().read_char(&Character::Char('q'))
        );
        assert_eq!("s", &text[s.read_char(&Character::Char('s')).unwrap()]);
        assert_eq!("d", &text[s.read_char(&Character::Char('d')).unwrap()]);
        assert_eq!("f", &text[s.read_char(&Character::Char('f')).unwrap()]);
        assert_eq!("", &text[s.read_eol().unwrap()]);
    }

    #[test]
    fn test_read_string() {
        let text = "asdf";
        let s = Scanner::new(text);
        assert_eq!(Ok("as"), s.read_string("as").map(|r| &text[r]));
        assert_eq!(
            Err(SyntaxError {
                range: 2..3,
                want: Token::Sequence(Sequence::One(Character::Char('q'))),
                context: None,
            }),
            s.clone().read_char(&Character::Char('q'))
        );
        assert_eq!(Ok("df"), s.read_string("df").map(|r| &text[r]));
        assert_eq!(Ok(""), s.read_eol().map(|r| &text[r]));
    }

    #[test]
    fn test_read_transaction() {
        let text = "asdf";
        let s = Scanner::new(text);
        let rollback = s.snapshot();

        assert_eq!(Ok("asdf"), s.read_string("asdf").map(|r| &text[r]));
        assert_eq!(s.current(), None);

        rollback();

        assert_eq!(s.current(), Some('a'));
        assert_eq!(Ok("asdf"), s.read_string("asdf").map(|r| &text[r]));
    }

    #[test]
    fn test_read_rest_of_line() {
        let text = "\n\n  \nfoo";
        let s = Scanner::new(text);
        assert_eq!(Ok("\n"), s.read_rest_of_line().map(|r| &text[r]));
        assert_eq!(Ok("\n"), s.read_rest_of_line().map(|r| &text[r]));
        assert_eq!(Ok("  \n"), s.read_rest_of_line().map(|r| &text[r]));
        assert_eq!(
            Err(SyntaxError {
                range: 5..6,
                want: Token::Sequence(Sequence::One(Character::OneOf(vec![
                    Character::NewLine,
                    Character::EOF
                ]))),
                context: None,
            }),
            s.clone().read_rest_of_line()
        );
        assert_eq!(Ok("foo"), s.read_string("foo").map(|r| &text[r]));
        assert_eq!(Ok(""), s.read_rest_of_line().map(|r| &text[r]));
    }

    #[test]
    fn test_read_sequence_number_of() {
        let text = "asdf";
        let s = Scanner::new(text);
        assert_eq!(
            Ok("as"),
            s.read_sequence(&Sequence::NumberOf(2, Character::Any))
                .map(|r| &text[r])
        );
        assert_eq!(
            Ok(""),
            s.read_sequence(&Sequence::NumberOf(0, Character::Any))
                .map(|r| &text[r])
        );
        assert_eq!(
            Err(SyntaxError {
                range: 2..4,
                want: Token::Sequence(Sequence::NumberOf(3, Character::Any)),
                context: None,
            }),
            s.read_sequence(&Sequence::NumberOf(3, Character::Any))
        );
        assert_eq!(Ok(""), s.read_eol().map(|r| &text[r]));
    }

    #[test]
    fn test_read_eol() {
        let text = "a\n\n";
        let s = Scanner::new(text);
        assert_eq!(
            Err(SyntaxError {
                range: 0..1,
                want: Token::Sequence(Sequence::One(Character::OneOf(vec![
                    Character::NewLine,
                    Character::EOF
                ]))),
                context: None,
            }),
            s.clone().read_eol()
        );
        assert_eq!(Some('a'), s.advance());
        assert_eq!(Ok("\n"), s.read_eol().map(|r| &text[r]));
        assert_eq!(Ok("\n"), s.read_eol().map(|r| &text[r]));
        assert_eq!(Ok(""), s.read_eol().map(|r| &text[r]));
        assert_eq!(Ok(""), s.read_eol().map(|r| &text[r]));
    }

    #[test]
    fn test_read_space_1() {
        let text = "  a\t\tb  \nc";
        let s = Scanner::new(text);

        assert_eq!(Ok("  "), s.read_space_1().map(|r| &text[r]));
        assert_eq!(Some('a'), s.advance());
        assert_eq!(Ok("\t\t"), s.read_space_1().map(|r| &text[r]));
        assert_eq!(
            Err(SyntaxError {
                range: 5..6,
                want: Token::Sequence(Sequence::One(Character::HorizontalSpace)),
                context: None,
            }),
            s.clone().read_space_1()
        );
        assert_eq!(Some('b'), s.advance());
        assert_eq!(Ok("  "), s.read_space_1().map(|r| &text[r]));
        assert_eq!(Ok(""), s.read_space_1().map(|r| &text[r]));
        assert_eq!(Ok("\n"), s.read_eol().map(|r| &text[r]));
        assert_eq!(Some('c'), s.advance());
        assert_eq!(Ok(""), s.read_eol().map(|r| &text[r]));
    }
}
