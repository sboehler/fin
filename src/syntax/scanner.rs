use super::cst::{Character, Rng, Sequence, Token};
use super::error::SyntaxError;
use std::{cell::RefCell, iter::Peekable, str::CharIndices};

#[derive(Clone)]
pub struct Scanner<'a> {
    pub source: &'a str,
    chars: RefCell<Peekable<CharIndices<'a>>>,
}

pub type Result<T> = std::result::Result<T, SyntaxError>;

struct Scope<'a, 'b> {
    s: &'a Scanner<'b>,
    start: usize,
}

impl Scope<'_, '_> {
    fn character_error(&self, want: &Character) -> SyntaxError {
        SyntaxError {
            rng: self.s.rng(self.start),
            want: Token::Sequence(Sequence::One(want.clone())),
            source: None,
        }
    }

    fn error(&self, want: &Sequence) -> SyntaxError {
        SyntaxError {
            rng: self.s.rng(self.start),
            want: Token::Sequence(want.clone()),
            source: None,
        }
    }

    fn rng(&self) -> Rng {
        self.start..self.s.pos()
    }
}

impl<'a> Scanner<'a> {
    pub fn new(text: &'a str) -> Scanner<'a> {
        Scanner {
            source: text,
            chars: RefCell::new(text.char_indices().peekable()),
        }
    }

    pub fn snapshot(&self) -> Box<dyn FnOnce() + '_> {
        let s = self.chars.borrow().clone();
        Box::new(|| {
            let _ = self.chars.replace(s);
        })
    }

    pub fn rng(&self, start: usize) -> Rng {
        start..self.pos()
    }

    fn scope(&self) -> Scope<'_, 'a> {
        Scope {
            s: self,
            start: self.pos(),
        }
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

    pub fn read_while_1(&self, ch: &Character) -> Result<Rng> {
        let scope = self.scope();
        if !ch.is(self.current()) {
            self.advance();
            return Err(scope.character_error(ch));
        }
        Ok(self.read_while(ch))
    }

    pub fn read_while(&self, ch: &Character) -> Rng {
        let scope = self.scope();
        while ch.is(self.current()) {
            self.advance();
        }
        scope.rng()
    }

    pub fn read_until(&self, ch: &Character) -> Rng {
        let scope = self.scope();
        while !ch.is(self.current()) {
            self.advance();
        }
        scope.rng()
    }

    pub fn read_all(&self) -> Rng {
        self.read_while(&Character::Any)
    }

    pub fn read_char(&self, ch: &Character) -> Result<Rng> {
        let scope = self.scope();
        let c = self.advance();
        if ch.is(c) {
            Ok(scope.rng())
        } else {
            Err(scope.character_error(ch))
        }
    }

    pub fn read_string(&self, str: &str) -> Result<Rng> {
        let scope = self.scope();
        for c in str.chars() {
            self.read_char(&Character::Char(c))?;
        }
        Ok(scope.rng())
    }

    pub fn read_sequence(&self, seq: &Sequence) -> Result<Rng> {
        let scope = self.scope();
        match seq {
            Sequence::One(ch) => {
                self.read_char(ch)?;
                Ok(scope.rng())
            }
            Sequence::OneOf(seqs) => {
                for s in seqs {
                    let rollback = self.snapshot();
                    if self.read_sequence(s).is_ok() {
                        return Ok(scope.rng());
                    }
                    rollback();
                }
                self.advance();
                Err(scope.error(seq))
            }
            Sequence::NumberOf(n, char) => {
                for _ in 0..*n {
                    self.read_char(char).map_err(|_| scope.error(seq))?;
                }
                Ok(scope.rng())
            }
            Sequence::String(s) => {
                for c in s.chars() {
                    self.read_char(&Character::Char(c))
                        .map_err(|_| scope.error(seq))?;
                }
                Ok(scope.rng())
            }
        }
    }

    pub fn read_1(&self) -> Result<Rng> {
        let scope = self.scope();
        match self.advance() {
            Some(_) => Ok(scope.rng()),
            None => Err(scope.character_error(&Character::Any)),
        }
    }

    pub fn read_eol(&self) -> Result<Rng> {
        let scope = self.scope();
        let c = self.advance();
        match c {
            None | Some('\n') => Ok(scope.rng()),
            _ => {
                Err(scope
                    .character_error(&Character::OneOf(vec![Character::NewLine, Character::EOF])))
            }
        }
    }

    pub fn read_space_1(&self) -> Result<Rng> {
        let scope = self.scope();
        match self.current() {
            Some(ch) if !ch.is_ascii_whitespace() => {
                self.advance();
                Err(scope.character_error(&Character::HorizontalSpace))
            }
            _ => Ok(self.read_space()),
        }
    }

    pub fn read_space(&self) -> Rng {
        self.read_while(&Character::HorizontalSpace)
    }

    pub fn read_rest_of_line(&self) -> Result<Rng> {
        let scope = self.scope();
        self.read_while(&Character::HorizontalSpace);
        self.read_eol()?;
        Ok(scope.rng())
    }
}

#[cfg(test)]
mod test_scanner {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_read_while() {
        let text = "aaasdff";
        let s = Scanner::new(&text);
        assert_eq!("aaasd", &text[s.read_while(&Character::NotChar('f'))]);
        assert_eq!("ff", &text[s.read_while(&Character::Char('f'))]);
        assert_eq!("", &text[s.read_while(&Character::Char('q'))]);
        assert_eq!(Ok(""), s.read_eol().map(|rng| &text[rng]));
    }

    #[test]
    fn test_read_while_1() {
        let text = "aaasdff";
        let s = Scanner::new(&text);
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
                rng: 7..7,
                want: Token::Sequence(Sequence::One(Character::Char('q'))),
                source: None,
            }),
            s.read_while_1(&Character::Char('q'))
        );
        assert_eq!("", &text[s.read_eol().unwrap()]);
    }

    #[test]
    fn test_read_char() {
        let text = "asdf";
        let s = Scanner::new(&text);
        assert_eq!("a", &text[s.read_char(&Character::Char('a')).unwrap()]);
        assert_eq!(
            Err(SyntaxError {
                rng: 1..2,
                want: Token::Sequence(Sequence::One(Character::Char('q'))),
                source: None,
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
        let s = Scanner::new(&text);
        assert_eq!(Ok("as"), s.read_string("as").map(|r| &text[r]));
        assert_eq!(
            Err(SyntaxError {
                rng: 2..3,
                want: Token::Sequence(Sequence::One(Character::Char('q'))),
                source: None,
            }),
            s.clone().read_char(&Character::Char('q'))
        );
        assert_eq!(Ok("df"), s.read_string("df").map(|r| &text[r]));
        assert_eq!(Ok(""), s.read_eol().map(|r| &text[r]));
    }

    #[test]
    fn test_read_transaction() {
        let text = "asdf";
        let s = Scanner::new(&text);
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
        let s = Scanner::new(&text);
        assert_eq!(Ok("\n"), s.read_rest_of_line().map(|r| &text[r]));
        assert_eq!(Ok("\n"), s.read_rest_of_line().map(|r| &text[r]));
        assert_eq!(Ok("  \n"), s.read_rest_of_line().map(|r| &text[r]));
        assert_eq!(
            Err(SyntaxError {
                rng: 5..6,
                want: Token::Sequence(Sequence::One(Character::OneOf(vec![
                    Character::NewLine,
                    Character::EOF
                ]))),
                source: None,
            }),
            s.clone().read_rest_of_line()
        );
        assert_eq!(Ok("foo"), s.read_string("foo").map(|r| &text[r]));
        assert_eq!(Ok(""), s.read_rest_of_line().map(|r| &text[r]));
    }

    #[test]
    fn test_read_1() {
        let text = "foo";
        let s = Scanner::new(&text);
        assert_eq!(Ok("f"), s.read_1().map(|r| &text[r]));
        assert_eq!(Ok("o"), s.read_1().map(|r| &text[r]));
        assert_eq!(Ok("o"), s.read_1().map(|r| &text[r]));
        assert_eq!(
            Err(SyntaxError {
                rng: 3..3,
                want: Token::Sequence(Sequence::One(Character::Any)),
                source: None,
            }),
            s.read_1()
        );
        assert_eq!("", &text[s.read_eol().unwrap()]);
    }

    #[test]
    fn test_read_sequence_number_of() {
        let text = "asdf";
        let s = Scanner::new(&text);
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
                rng: 2..4,
                want: Token::Sequence(Sequence::NumberOf(3, Character::Any)),
                source: None,
            }),
            s.read_sequence(&Sequence::NumberOf(3, Character::Any))
        );
        assert_eq!(Ok(""), s.read_eol().map(|r| &text[r]));
    }

    #[test]
    fn test_read_eol() {
        let text = "a\n\n";
        let s = Scanner::new(&text);
        assert_eq!(
            Err(SyntaxError {
                rng: 0..1,
                want: Token::Sequence(Sequence::One(Character::OneOf(vec![
                    Character::NewLine,
                    Character::EOF
                ]))),
                source: None,
            }),
            s.clone().read_eol()
        );
        assert_eq!(Ok("a"), s.read_1().map(|r| &text[r]));
        assert_eq!(Ok("\n"), s.read_eol().map(|r| &text[r]));
        assert_eq!(Ok("\n"), s.read_eol().map(|r| &text[r]));
        assert_eq!(Ok(""), s.read_eol().map(|r| &text[r]));
        assert_eq!(Ok(""), s.read_eol().map(|r| &text[r]));
    }

    #[test]
    fn test_read_space_1() {
        let text = "  a\t\tb  \nc";
        let s = Scanner::new(&text);

        assert_eq!(Ok("  "), s.read_space_1().map(|r| &text[r]));
        assert_eq!(Ok("a"), s.read_1().map(|r| &text[r]));
        assert_eq!(Ok("\t\t"), s.read_space_1().map(|r| &text[r]));
        assert_eq!(
            Err(SyntaxError {
                rng: 5..6,
                want: Token::Sequence(Sequence::One(Character::HorizontalSpace)),
                source: None,
            }),
            s.clone().read_space_1()
        );
        assert_eq!(Ok("b"), s.read_1().map(|r| &text[r]));
        assert_eq!(Ok("  "), s.read_space_1().map(|r| &text[r]));
        assert_eq!(Ok(""), s.read_space_1().map(|r| &text[r]));
        assert_eq!(Ok("\n"), s.read_eol().map(|r| &text[r]));
        assert_eq!(Ok("c"), s.read_1().map(|r| &text[r]));
        assert_eq!(Ok(""), s.read_eol().map(|r| &text[r]));
    }
}
