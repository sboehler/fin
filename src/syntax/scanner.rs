use super::cst::Rng;
use super::file::File;
use std::rc::Rc;
use std::{cell::RefCell, iter::Peekable, str::CharIndices};
use thiserror::Error;

#[derive(Clone)]
pub struct Scanner<'a> {
    pub source: &'a Rc<File>,
    chars: RefCell<Peekable<CharIndices<'a>>>,
}

#[derive(Error, Debug, Eq, PartialEq)]
#[error("syntax error")]
pub struct ScannerError {
    pub rng: Rng,
    pub want: Character,
}

pub type Result<T> = std::result::Result<T, ScannerError>;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Character {
    EOF,
    Char(char),
    NotChar(char),
    Digit,
    Alphabetic,
    AlphaNum,
    Any,
    HorizontalSpace,
    NewLine,
    OneOf(Vec<Character>),
}

impl Character {
    pub fn from_char(ch: Option<char>) -> Self {
        match ch {
            None => Self::EOF,
            Some('\n') => Self::NewLine,
            Some(c) if c.is_whitespace() => Self::HorizontalSpace,
            Some(c) => Self::Char(c),
        }
    }

    pub fn is(&self, o: Option<char>) -> bool {
        match o {
            None => match self {
                Character::EOF => true,
                Character::NewLine => true,
                _ => false,
            },
            Some(c) => match self {
                Character::EOF => false,
                Character::Char(a) => c == *a,
                Character::NotChar(a) => c != *a,
                Character::Digit => c.is_ascii_digit(),
                Character::Alphabetic => c.is_alphabetic(),
                Character::AlphaNum => c.is_alphanumeric(),
                Character::Any => true,
                Character::HorizontalSpace => c.is_ascii_whitespace() && c != '\n',
                Character::NewLine => c == '\n',
                Character::OneOf(cs) => cs.iter().any(|c| c.is(o)),
            },
        }
    }
}

struct Scope<'a, 'b> {
    s: &'a Scanner<'b>,
    start: usize,
}

impl<'a, 'b> Scope<'a, 'b> {
    fn error(&self, want: &Character) -> ScannerError {
        ScannerError {
            rng: self.s.rng(self.start),
            want: want.clone(),
        }
    }
    fn rng(&self) -> Rng {
        Rng::new(self.s.source.clone(), self.start, self.s.pos())
    }
}

impl<'a> Scanner<'a> {
    pub fn new(s: &'a Rc<File>) -> Scanner<'a> {
        Scanner {
            source: s,
            chars: RefCell::new(s.text.char_indices().peekable()),
        }
    }

    pub fn rng(&self, start: usize) -> Rng {
        Rng {
            file: self.source.clone(),
            start,
            end: self.pos(),
        }
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
            .map_or_else(|| self.source.text.len(), |t| t.0)
    }

    pub fn read_while_1(&self, ch: &Character) -> Result<Rng> {
        let scope = self.scope();
        if !ch.is(self.current()) {
            self.advance();
            return Err(scope.error(&ch));
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
            Err(scope.error(ch))
        }
    }

    pub fn read_string(&self, str: &str) -> Result<Rng> {
        let scope = self.scope();
        for c in str.chars() {
            self.read_char(&Character::Char(c))?;
        }
        Ok(scope.rng())
    }

    pub fn read_1(&self) -> Result<Rng> {
        let scope = self.scope();
        match self.advance() {
            Some(_) => Ok(scope.rng()),
            None => Err(scope.error(&Character::Any)),
        }
    }

    pub fn read_n(&self, n: usize, token: Character) -> Result<Rng> {
        let scope = self.scope();
        for _ in 0..n {
            self.read_char(&token)?;
        }
        Ok(scope.rng())
    }

    pub fn read_eol(&self) -> Result<Rng> {
        let scope = self.scope();
        let c = self.advance();
        match c {
            None | Some('\n') => Ok(scope.rng()),
            _ => Err(scope.error(&Character::OneOf(vec![
                Character::Char('\n'),
                Character::EOF,
            ]))),
        }
    }

    pub fn read_space_1(&self) -> Result<Rng> {
        let scope = self.scope();
        match self.current() {
            Some(ch) if !ch.is_ascii_whitespace() => {
                self.advance();
                Err(scope.error(&Character::HorizontalSpace))
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
        let mem = File::mem("aaasdff");
        let s = Scanner::new(&mem);
        assert_eq!("aaasd", s.read_while(&Character::NotChar('f')).text());
        assert_eq!("ff", s.read_while(&Character::Char('f')).text());
        assert_eq!("", s.read_while(&Character::Char('q')).text());
        assert_eq!(Ok(""), s.read_eol().as_ref().map(Rng::text));
    }

    #[test]
    fn test_read_while_1() {
        let f = File::mem("aaasdff");
        let s = Scanner::new(&f);
        assert_eq!(
            Ok("aaasd"),
            s.read_while_1(&Character::NotChar('f'))
                .as_ref()
                .map(Rng::text)
        );
        assert_eq!(
            Ok("ff"),
            s.read_while_1(&Character::Char('f'))
                .as_ref()
                .map(Rng::text)
        );
        assert_eq!(
            Err(ScannerError {
                rng: Rng::new(f.clone(), 7, 7),
                want: Character::Char('q'),
            }),
            s.read_while_1(&Character::Char('q'))
        );
        assert_eq!("", s.read_eol().unwrap().text());
    }

    #[test]
    fn test_read_char() {
        let f = File::mem("asdf");
        let s = Scanner::new(&f);
        assert_eq!("a", s.read_char(&Character::Char('a')).unwrap().text());
        assert_eq!(
            Err(ScannerError {
                rng: Rng::new(f.clone(), 1, 2),
                want: Character::Char('q'),
            }),
            s.clone().read_char(&Character::Char('q'))
        );
        assert_eq!("s", s.read_char(&Character::Char('s')).unwrap().text());
        assert_eq!("d", s.read_char(&Character::Char('d')).unwrap().text());
        assert_eq!("f", s.read_char(&Character::Char('f')).unwrap().text());
        assert_eq!("", s.read_eol().unwrap().text());
    }

    #[test]
    fn test_read_string() {
        let f = File::mem("asdf");
        let s = Scanner::new(&f);
        assert_eq!(Ok("as"), s.read_string("as").as_ref().map(Rng::text));
        assert_eq!(
            Err(ScannerError {
                rng: Rng::new(f.clone(), 2, 3),
                want: Character::Char('q'),
            }),
            s.clone().read_char(&Character::Char('q'))
        );
        assert_eq!(Ok("df"), s.read_string("df").as_ref().map(Rng::text));
        assert_eq!(Ok(""), s.read_eol().as_ref().map(Rng::text));
    }

    #[test]
    fn test_read_rest_of_line() {
        let f = File::mem("\n\n  \nfoo");
        let s = Scanner::new(&f);
        assert_eq!(Ok("\n"), s.read_rest_of_line().as_ref().map(Rng::text));
        assert_eq!(Ok("\n"), s.read_rest_of_line().as_ref().map(Rng::text));
        assert_eq!(Ok("  \n"), s.read_rest_of_line().as_ref().map(Rng::text));
        assert_eq!(
            Err(ScannerError {
                rng: Rng::new(f.clone(), 5, 6),
                want: Character::OneOf(vec![Character::Char('\n'), Character::EOF]),
            }),
            s.clone().read_rest_of_line()
        );
        assert_eq!(Ok("foo"), s.read_string("foo").as_ref().map(Rng::text));
        assert_eq!(Ok(""), s.read_rest_of_line().as_ref().map(Rng::text));
    }

    #[test]
    fn test_read_1() {
        let f = File::mem("foo");
        let s = Scanner::new(&f);
        assert_eq!(Ok("f"), s.read_1().as_ref().map(Rng::text));
        assert_eq!(Ok("o"), s.read_1().as_ref().map(Rng::text));
        assert_eq!(Ok("o"), s.read_1().as_ref().map(Rng::text));
        assert_eq!(
            Err(ScannerError {
                rng: Rng::new(f.clone(), 3, 3),
                want: Character::Any,
            }),
            s.read_1()
        );
        assert_eq!("", s.read_eol().unwrap().text());
    }

    #[test]
    fn test_read_n() {
        let f = File::mem("asdf");
        let s = Scanner::new(&f);
        assert_eq!(
            Ok("as"),
            s.read_n(2, Character::Any).as_ref().map(Rng::text)
        );
        assert_eq!(Ok(""), s.read_n(0, Character::Any).as_ref().map(Rng::text));
        assert_eq!(
            Err(ScannerError {
                rng: Rng::new(f.clone(), 4, 4),
                want: Character::Any,
            }),
            s.read_n(3, Character::Any)
        );
        assert_eq!(Ok(""), s.read_eol().as_ref().map(Rng::text));
    }

    #[test]
    fn test_read_eol() {
        let f = File::mem("a\n\n");
        let s = Scanner::new(&f);
        assert_eq!(
            Err(ScannerError {
                rng: Rng::new(f.clone(), 0, 1),
                want: Character::OneOf(vec![Character::Char('\n'), Character::EOF]),
            }),
            s.clone().read_eol()
        );
        assert_eq!(Ok("a"), s.read_1().as_ref().map(Rng::text));
        assert_eq!(Ok("\n"), s.read_eol().as_ref().map(Rng::text));
        assert_eq!(Ok("\n"), s.read_eol().as_ref().map(Rng::text));
        assert_eq!(Ok(""), s.read_eol().as_ref().map(Rng::text));
        assert_eq!(Ok(""), s.read_eol().as_ref().map(Rng::text));
    }

    #[test]
    fn test_read_space_1() {
        let f = File::mem("  a\t\tb  \nc");
        let s = Scanner::new(&f);

        assert_eq!(Ok("  "), s.read_space_1().as_ref().map(Rng::text));
        assert_eq!(Ok("a"), s.read_1().as_ref().map(Rng::text));
        assert_eq!(Ok("\t\t"), s.read_space_1().as_ref().map(Rng::text));
        assert_eq!(
            Err(ScannerError {
                rng: Rng::new(f.clone(), 5, 6),
                want: Character::HorizontalSpace,
            }),
            s.clone().read_space_1()
        );
        assert_eq!(Ok("b"), s.read_1().as_ref().map(Rng::text));
        assert_eq!(Ok("  "), s.read_space_1().as_ref().map(Rng::text));
        assert_eq!(Ok(""), s.read_space_1().as_ref().map(Rng::text));
        assert_eq!(Ok("\n"), s.read_eol().as_ref().map(Rng::text));
        assert_eq!(Ok("c"), s.read_1().as_ref().map(Rng::text));
        assert_eq!(Ok(""), s.read_eol().as_ref().map(Rng::text));
    }
}
