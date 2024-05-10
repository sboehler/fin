use super::cst::{Rng, Token};
use super::file::File;
use std::rc::Rc;
use std::{cell::RefCell, iter::Peekable, str::CharIndices};
use thiserror::Error;

pub struct Scanner<'a> {
    source: &'a Rc<File>,
    chars: RefCell<Peekable<CharIndices<'a>>>,
}

#[derive(Error, Debug, Eq, PartialEq)]
#[error("syntax error")]
pub struct ScannerError {
    pub file: Rc<File>,
    pub pos: usize,
    pub msg: Option<String>,
    pub want: Token,
    pub got: Token,
}

pub type Result<T> = std::result::Result<T, ScannerError>;

struct Scope<'a, 'b> {
    s: &'a Scanner<'b>,
    start: usize,
    msg: Option<&'a str>,
}

impl<'a, 'b> Scope<'a, 'b> {
    fn error(&self, want: Token, got: Token) -> ScannerError {
        ScannerError {
            file: self.s.source.clone(),
            pos: self.s.pos(),
            msg: self.msg.map(|s| s.to_string()),
            want,
            got,
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
            msg: None,
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

    pub fn read_while_1<P>(&self, token: Token, pred: P) -> Result<Rng>
    where
        P: Fn(char) -> bool,
    {
        let scope = self.scope();
        if !self.current().map_or(false, &pred) {
            return Err(scope.error(token, Token::from_char(self.current())));
        }
        Ok(self.read_while(pred))
    }

    pub fn read_while<P>(&self, pred: P) -> Rng
    where
        P: Fn(char) -> bool,
    {
        let scope = self.scope();
        while self.current().map_or(false, &pred) {
            self.advance();
        }
        scope.rng()
    }

    pub fn read_until<P>(&self, pred: P) -> Rng
    where
        P: Fn(char) -> bool,
    {
        self.read_while(|v| !pred(v))
    }

    pub fn read_all(&self) -> Rng {
        self.read_while(|_| true)
    }

    pub fn read_char(&self, c: char) -> Result<Rng> {
        let scope = self.scope();
        match self.current() {
            Some(d) if c == d => {
                self.advance();
                Ok(scope.rng())
            }
            o => Err(scope.error(Token::Char(c), Token::from_char(o))),
        }
    }

    pub fn read_string(&self, str: &str) -> Result<Rng> {
        let scope = self.scope();
        for c in str.chars() {
            self.read_char(c)?;
        }
        Ok(scope.rng())
    }

    pub fn read_identifier(&self) -> Result<Rng> {
        let scope = self.scope();
        if self.read_while(char::is_alphanumeric).is_empty() {
            Err(scope.error(Token::AlphaNum, Token::from_char(self.current())))
        } else {
            Ok(scope.rng())
        }
    }

    pub fn read_1(&self) -> Result<Rng> {
        let scope = self.scope();
        match self.advance() {
            Some(_) => Ok(scope.rng()),
            None => Err(scope.error(Token::Any, Token::EOF)),
        }
    }

    pub fn read_1_with<P>(&self, token: Token, pred: P) -> Result<Rng>
    where
        P: Fn(char) -> bool,
    {
        let scope = self.scope();
        match self.current() {
            Some(c) if pred(c) => {
                self.advance();
                Ok(scope.rng())
            }
            Some(c) => Err(scope.error(token, Token::Char(c))),
            None => Err(scope.error(token, Token::EOF)),
        }
    }

    pub fn read_n_with<P>(&self, n: usize, token: Token, pred: P) -> Result<Rng>
    where
        P: Fn(char) -> bool,
    {
        let scope = self.scope();
        for _ in 0..n {
            match self.current() {
                Some(c) if pred(c) => self.advance(),
                Some(c) => return Err(scope.error(token, Token::Char(c))),
                None => return Err(scope.error(token, Token::EOF)),
            };
        }
        Ok(scope.rng())
    }

    pub fn read_n(&self, n: usize) -> Result<Rng> {
        let scope = self.scope();
        for _ in 0..n {
            self.read_1()?;
        }
        Ok(scope.rng())
    }

    pub fn read_eol(&self) -> Result<Rng> {
        let scope = self.scope();
        match self.current() {
            None | Some('\n') => {
                self.advance();
                Ok(scope.rng())
            }
            Some(ch) => Err(self.error(
                None,
                Token::Either(vec![Token::Char('\n'), Token::EOF]),
                Token::Char(ch),
            )),
        }
    }

    pub fn read_space1(&self) -> Result<Rng> {
        let scope = self.scope();
        match self.current() {
            Some(ch) if !ch.is_ascii_whitespace() => {
                Err(scope.error(Token::WhiteSpace, Token::Char(ch)))
            }
            _ => Ok(self.read_space()),
        }
    }

    pub fn read_space(&self) -> Rng {
        self.read_while(|c| c != '\n' && c.is_ascii_whitespace())
    }

    pub fn read_rest_of_line(&self) -> Result<Rng> {
        let scope = self.scope();
        self.read_while(|c| c.is_whitespace() && c != '\n');
        self.read_eol()?;
        Ok(scope.rng())
    }

    pub fn error(&self, msg: Option<String>, want: Token, got: Token) -> ScannerError {
        ScannerError {
            file: self.source.clone(),
            pos: self.pos(),
            msg,
            want,
            got,
        }
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
        assert_eq!("aaasd", s.read_while(|c| c != 'f').text());
        assert_eq!("ff", s.read_while(|c| c == 'f').text());
        assert_eq!("", s.read_while(|c| c == 'q').text());
        assert_eq!(Ok(""), s.read_eol().as_ref().map(Rng::text));
    }

    #[test]
    fn test_read_while_1() {
        let f = File::mem("aaasdff");
        let s = Scanner::new(&f);
        assert_eq!(
            Ok("aaasd"),
            s.read_while_1(Token::Any, |c| c != 'f')
                .as_ref()
                .map(Rng::text)
        );
        assert_eq!(
            Ok("ff"),
            s.read_while_1(Token::Char('f'), |c| c == 'f')
                .as_ref()
                .map(Rng::text)
        );
        assert_eq!(
            Err(ScannerError {
                file: f.clone(),
                pos: 7,
                msg: None,
                want: Token::Char('q'),
                got: Token::EOF
            }),
            s.read_while_1(Token::Char('q'), |c| c == 'q')
        );
        assert_eq!("", s.read_eol().unwrap().text());
    }

    #[test]
    fn test_read_char() {
        let f = File::mem("asdf");
        let s = Scanner::new(&f);
        assert_eq!("a", s.read_char('a').unwrap().text());
        assert_eq!(
            Err(ScannerError {
                file: f.clone(),
                pos: 1,
                msg: None,
                want: Token::Char('q'),
                got: Token::Char('s')
            }),
            s.read_char('q')
        );
        assert_eq!("s", s.read_char('s').unwrap().text());
        assert_eq!("d", s.read_char('d').unwrap().text());
        assert_eq!("f", s.read_char('f').unwrap().text());
        assert_eq!("", s.read_eol().unwrap().text());
    }

    #[test]
    fn test_read_string() {
        let f = File::mem("asdf");
        let s = Scanner::new(&f);
        assert_eq!(Ok("as"), s.read_string("as").as_ref().map(Rng::text));
        assert_eq!(
            Err(ScannerError {
                file: s.source.clone(),
                pos: 2,
                msg: None,
                want: Token::Char('q'),
                got: Token::Char('d')
            }),
            s.read_char('q')
        );
        assert_eq!(Ok("df"), s.read_string("df").as_ref().map(Rng::text));
        assert_eq!(Ok(""), s.read_eol().as_ref().map(Rng::text));
    }

    #[test]
    fn test_read_identifier() {
        let f = File::mem("foo bar 1baz");
        let s = Scanner::new(&f);
        assert_eq!(Ok("foo"), s.read_identifier().as_ref().map(Rng::text));
        assert_eq!(" ", s.read_while(|c| c.is_ascii_whitespace()).text());
        assert_eq!(Ok("bar"), s.read_identifier().as_ref().map(Rng::text));
        assert_eq!(" ", s.read_while(|c| c.is_ascii_whitespace()).text());
        assert_eq!(Ok("1baz"), s.read_identifier().as_ref().map(Rng::text));
        assert_eq!(Ok(""), s.read_eol().as_ref().map(Rng::text));
    }

    #[test]
    fn read_rest_of_line() {
        let f = File::mem("\n\n  \nfoo");
        let s = Scanner::new(&f);
        assert_eq!(Ok("\n"), s.read_rest_of_line().as_ref().map(Rng::text));
        assert_eq!(Ok("\n"), s.read_rest_of_line().as_ref().map(Rng::text));
        assert_eq!(Ok("  \n"), s.read_rest_of_line().as_ref().map(Rng::text));
        assert_eq!(
            Err(ScannerError {
                file: File::mem("\n\n  \nfoo"),
                pos: 5,
                msg: None,
                want: Token::Either(vec![Token::Char('\n'), Token::EOF]),
                got: Token::Char('f')
            }),
            s.read_rest_of_line()
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
                file: s.source.clone(),
                pos: 3,
                msg: None,
                want: Token::Any,
                got: Token::EOF
            }),
            s.read_1()
        );
        assert_eq!("", s.read_eol().unwrap().text());
    }

    #[test]
    fn test_read_1_with() {
        let f = File::mem("asdf");
        let s = Scanner::new(&f);
        assert_eq!(
            "a",
            s.read_1_with(Token::Char('a'), |c| c == 'a')
                .unwrap()
                .text()
        );
        assert_eq!(
            "s",
            s.read_1_with(Token::Custom("no a".into()), |c| c != 'a')
                .unwrap()
                .text()
        );
        assert_eq!(
            Err(ScannerError {
                file: s.source.clone(),
                pos: 2,
                msg: None,
                want: Token::Digit,
                got: Token::Char('d')
            }),
            s.read_1_with(Token::Digit, |c| c.is_ascii_digit())
        );
        assert_eq!(
            Ok("d"),
            s.read_1_with(Token::Char('d'), |c| c == 'd')
                .as_ref()
                .map(Rng::text)
        );
        assert_eq!(
            Ok("f"),
            s.read_1_with(Token::Char('f'), |c| c == 'f')
                .as_ref()
                .map(Rng::text)
        );
        assert_eq!(
            Err(ScannerError {
                file: s.source.clone(),
                pos: 4,
                msg: None,
                want: Token::Any,
                got: Token::EOF
            }),
            s.read_1_with(Token::Any, |_| true)
        );
        assert_eq!(Ok(""), s.read_eol().as_ref().map(Rng::text));
    }

    #[test]
    fn test_read_n() {
        let f = File::mem("asdf");
        let s = Scanner::new(&f);
        assert_eq!(Ok("as"), s.read_n(2).as_ref().map(Rng::text));
        assert_eq!(Ok(""), s.read_n(0).as_ref().map(Rng::text));
        assert_eq!(
            Err(ScannerError {
                file: s.source.clone(),
                pos: 4,
                msg: None,
                want: Token::Any,
                got: Token::EOF
            }),
            s.read_n(3)
        );
        assert_eq!(Ok(""), s.read_eol().as_ref().map(Rng::text));
    }

    #[test]
    fn test_read_eol() {
        let f = File::mem("a\n\n");
        let s = Scanner::new(&f);
        assert_eq!(
            Err(ScannerError {
                file: s.source.clone(),
                pos: 0,
                msg: None,
                want: Token::Either(vec![Token::Char('\n'), Token::EOF]),
                got: Token::Char('a')
            }),
            s.read_eol()
        );
        assert_eq!(Ok("a"), s.read_1().as_ref().map(Rng::text));
        assert_eq!(Ok("\n"), s.read_eol().as_ref().map(Rng::text));
        assert_eq!(Ok("\n"), s.read_eol().as_ref().map(Rng::text));
        assert_eq!(Ok(""), s.read_eol().as_ref().map(Rng::text));
        assert_eq!(Ok(""), s.read_eol().as_ref().map(Rng::text));
    }

    #[test]
    fn test_read_space1() {
        let f = File::mem("  a\t\tb  \nc");
        let s = Scanner::new(&f);

        assert_eq!(Ok("  "), s.read_space1().as_ref().map(Rng::text));
        assert_eq!(Ok("a"), s.read_1().as_ref().map(Rng::text));
        assert_eq!(Ok("\t\t"), s.read_space1().as_ref().map(Rng::text));
        assert_eq!(
            Err(ScannerError {
                file: s.source.clone(),
                pos: 5,
                msg: None,
                want: Token::WhiteSpace,
                got: Token::Char('b')
            }),
            s.read_space1()
        );
        assert_eq!(Ok("b"), s.read_1().as_ref().map(Rng::text));
        assert_eq!(Ok("  "), s.read_space1().as_ref().map(Rng::text));
        assert_eq!(Ok(""), s.read_space1().as_ref().map(Rng::text));
        assert_eq!(Ok("\n"), s.read_eol().as_ref().map(Rng::text));
        assert_eq!(Ok("c"), s.read_1().as_ref().map(Rng::text));
        assert_eq!(Ok(""), s.read_eol().as_ref().map(Rng::text));
    }
}
