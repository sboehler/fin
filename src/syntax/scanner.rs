use super::cst::{Rng, Token};
use super::error::SyntaxError;
use std::{cell::RefCell, iter::Peekable, str::CharIndices};

pub struct Scanner<'a> {
    pub source: &'a str,
    chars: RefCell<Peekable<CharIndices<'a>>>,
}

pub type Result<T> = std::result::Result<T, SyntaxError>;

impl<'a> Scanner<'a> {
    pub fn new(s: &'a str) -> Scanner<'a> {
        Scanner {
            source: s,
            chars: RefCell::new(s.char_indices().peekable()),
        }
    }

    pub fn rng(&self, start: usize) -> Rng {
        Rng {
            start,
            end: self.pos(),
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
            .map_or_else(|| self.source.as_bytes().len(), |t| t.0)
    }

    pub fn read_while_1<P>(&self, token: Token, pred: P) -> Result<Rng>
    where
        P: Fn(char) -> bool,
    {
        if !self.current().map_or(false, &pred) {
            return Err(self.error(self.pos(), None, token, Token::from_char(self.current())));
        }
        Ok(self.read_while(pred))
    }

    pub fn read_while<P>(&self, pred: P) -> Rng
    where
        P: Fn(char) -> bool,
    {
        let start = self.pos();
        while self.current().map_or(false, &pred) {
            self.advance();
        }
        self.rng(start)
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
        let start = self.pos();
        match self.current() {
            Some(d) if c == d => {
                self.advance();
                Ok(self.rng(start))
            }
            o => Err(self.error(self.pos(), None, Token::Char(c), Token::from_char(o))),
        }
    }

    pub fn read_string(&self, str: &str) -> Result<Rng> {
        let start = self.pos();
        for c in str.chars() {
            self.read_char(c)?;
        }
        Ok(self.rng(start))
    }

    pub fn read_identifier(&self) -> Result<Rng> {
        let start = self.pos();
        if self.read_while(char::is_alphanumeric).is_empty() {
            Err(self.error(
                start,
                Some("parsing identifier".into()),
                Token::AlphaNum,
                Token::from_char(self.current()),
            ))
        } else {
            Ok(self.rng(start))
        }
    }

    pub fn read_1(&self) -> Result<Rng> {
        let start = self.pos();
        match self.advance() {
            Some(_) => Ok(self.rng(start)),
            None => Err(self.error(start, None, Token::Any, Token::EOF)),
        }
    }

    pub fn read_1_with<P>(&self, token: Token, pred: P) -> Result<Rng>
    where
        P: Fn(char) -> bool,
    {
        let start = self.pos();
        match self.current() {
            Some(c) if pred(c) => {
                self.advance();
                Ok(self.rng(start))
            }
            Some(c) => Err(self.error(start, None, token, Token::Char(c))),
            None => Err(self.error(start, None, token, Token::EOF)),
        }
    }

    pub fn read_n_with<P>(&self, n: usize, token: Token, pred: P) -> Result<Rng>
    where
        P: Fn(char) -> bool,
    {
        let start = self.pos();
        for _ in 0..n {
            match self.current() {
                Some(c) if pred(c) => self.advance(),
                Some(c) => return Err(self.error(self.pos(), None, token, Token::Char(c))),
                None => return Err(self.error(self.pos(), None, token, Token::EOF)),
            };
        }
        Ok(self.rng(start))
    }

    pub fn read_n(&self, n: usize) -> Result<Rng> {
        let start = self.pos();
        for _ in 0..n {
            self.read_1()?;
        }
        Ok(self.rng(start))
    }

    pub fn read_eol(&self) -> Result<Rng> {
        let start = self.pos();
        match self.current() {
            None | Some('\n') => {
                self.advance();
                Ok(self.rng(start))
            }
            Some(ch) => Err(self.error(
                start,
                None,
                Token::Either(vec![Token::Char('\n'), Token::EOF]),
                Token::Char(ch),
            )),
        }
    }

    pub fn read_space1(&self) -> Result<Rng> {
        let start = self.pos();
        match self.current() {
            Some(ch) if !ch.is_ascii_whitespace() => {
                Err(self.error(start, None, Token::WhiteSpace, Token::Char(ch)))
            }
            _ => Ok(self.read_space()),
        }
    }

    pub fn read_space(&self) -> Rng {
        self.read_while(|c| c != '\n' && c.is_ascii_whitespace())
    }

    pub fn read_rest_of_line(&self) -> Result<Rng> {
        let start = self.pos();
        self.read_while(|c| c.is_whitespace() && c != '\n');
        self.read_eol()?;
        Ok(self.rng(start))
    }

    fn error(&self, pos: usize, msg: Option<String>, want: Token, got: Token) -> SyntaxError {
        SyntaxError::new(self.source, pos, msg, want, got)
    }
}

#[cfg(test)]
mod test_scanner {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_read_while() {
        let s = Scanner::new("aaasdff");
        assert_eq!(Rng::new(0, "aaasd"), s.read_while(|c| c != 'f'));
        assert_eq!(Rng::new(5, "ff"), s.read_while(|c| c == 'f'));
        assert_eq!(Rng::new(7, ""), s.read_while(|c| c == 'q'));
        assert_eq!(Ok(Rng::new(7, "")), s.read_eol());
    }

    #[test]
    fn test_read_while_1() {
        let s = Scanner::new("aaasdff");
        assert_eq!(
            Ok(Rng::new(0, "aaasd")),
            s.read_while_1(Token::Any, |c| c != 'f')
        );
        assert_eq!(
            Ok(Rng::new(5, "ff")),
            s.read_while_1(Token::Char('f'), |c| c == 'f')
        );
        assert_eq!(
            Err(SyntaxError::new(
                "aaasdff",
                7,
                None,
                Token::Char('q'),
                Token::EOF
            )),
            s.read_while_1(Token::Char('q'), |c| c == 'q')
        );
        assert_eq!(Ok(Rng::new(7, "")), s.read_eol());
    }

    #[test]
    fn test_read_char() {
        let s = Scanner::new("asdf");
        assert_eq!(Ok(Rng::new(0, "a")), s.read_char('a'));
        assert_eq!(
            Err(SyntaxError::new(
                "asdf",
                1,
                None,
                Token::Char('q'),
                Token::Char('s')
            )),
            s.read_char('q')
        );
        assert_eq!(Ok(Rng::new(1, "s")), s.read_char('s'));
        assert_eq!(Ok(Rng::new(2, "d")), s.read_char('d'));
        assert_eq!(Ok(Rng::new(3, "f")), s.read_char('f'));
        assert_eq!(Ok(Rng::new(4, "")), s.read_eol());
    }

    #[test]
    fn test_read_string() {
        let s = Scanner::new("asdf");

        assert_eq!(Ok(Rng::new(0, "as")), s.read_string("as"),);
        assert_eq!(
            Err(SyntaxError::new(
                "asdf",
                2,
                None,
                Token::Char('q'),
                Token::Char('d')
            )),
            s.read_char('q')
        );
        assert_eq!(Ok(Rng::new(2, "df")), s.read_string("df"));
        assert_eq!(Ok(Rng::new(4, "")), s.read_eol());
    }

    #[test]
    fn test_read_identifier() {
        let s = Scanner::new("foo bar 1baz");
        assert_eq!(Ok(Rng::new(0, "foo")), s.read_identifier());
        assert_eq!(Rng::new(3, " "), s.read_while(|c| c.is_ascii_whitespace()));
        assert_eq!(Ok(Rng::new(4, "bar")), s.read_identifier());
        assert_eq!(Rng::new(7, " "), s.read_while(|c| c.is_ascii_whitespace()));
        assert_eq!(Ok(Rng::new(8, "1baz")), s.read_identifier());
        assert_eq!(Ok(Rng::new(12, "")), s.read_eol());
    }

    #[test]
    fn read_rest_of_line() {
        let s = Scanner::new("\n\n  \nfoo");
        assert_eq!(Ok(Rng::new(0, "\n")), s.read_rest_of_line());
        assert_eq!(Ok(Rng::new(1, "\n")), s.read_rest_of_line());
        assert_eq!(Ok(Rng::new(2, "  \n")), s.read_rest_of_line());
        assert_eq!(
            Err(SyntaxError::new(
                "\n\n  \nfoo",
                5,
                None,
                Token::Either(vec![Token::Char('\n'), Token::EOF]),
                Token::Char('f')
            )),
            s.read_rest_of_line()
        );
        assert_eq!(Ok(Rng::new(5, "foo")), s.read_string("foo"));
        assert_eq!(Ok(Rng::new(8, "")), s.read_rest_of_line());
    }

    #[test]
    fn test_read_1() {
        let s = Scanner::new("foo");
        assert_eq!(Ok(Rng::new(0, "f")), s.read_1());
        assert_eq!(Ok(Rng::new(1, "o")), s.read_1());
        assert_eq!(Ok(Rng::new(2, "o")), s.read_1());
        assert_eq!(
            Err(SyntaxError::new("foo", 3, None, Token::Any, Token::EOF)),
            s.read_1()
        );
        assert_eq!(Ok(Rng::new(3, "")), s.read_eol());
    }

    #[test]
    fn test_read_1_with() {
        let s = Scanner::new("asdf");
        assert_eq!(
            Ok(Rng::new(0, "a")),
            s.read_1_with(Token::Char('a'), |c| c == 'a'),
        );
        assert_eq!(
            Ok(Rng::new(1, "s")),
            s.read_1_with(Token::Custom("no a".into()), |c| c != 'a')
        );
        assert_eq!(
            Err(SyntaxError::new(
                "asdf",
                2,
                None,
                Token::Digit,
                Token::Char('d')
            )),
            s.read_1_with(Token::Digit, |c| c.is_ascii_digit())
        );
        assert_eq!(
            Ok(Rng::new(2, "d")),
            s.read_1_with(Token::Char('d'), |c| c == 'd')
        );
        assert_eq!(
            Ok(Rng::new(3, "f")),
            s.read_1_with(Token::Char('f'), |c| c == 'f')
        );
        assert_eq!(
            Err(SyntaxError::new("asdf", 4, None, Token::Any, Token::EOF)),
            s.read_1_with(Token::Any, |_| true)
        );
        assert_eq!(Ok(Rng::new(4, "")), s.read_eol());
    }

    #[test]
    fn test_read_n() {
        let s = Scanner::new("asdf");
        assert_eq!(Ok(Rng::new(0, "as")), s.read_n(2));
        assert_eq!(Ok(Rng::new(2, "")), s.read_n(0));
        assert_eq!(
            Err(SyntaxError::new("asdf", 4, None, Token::Any, Token::EOF)),
            s.read_n(3)
        );
        assert_eq!(Ok(Rng::new(4, "")), s.read_eol());
    }

    #[test]
    fn test_read_eol() {
        let s = Scanner::new("a\n\n");
        assert_eq!(
            Err(SyntaxError::new(
                "a\n\n",
                0,
                None,
                Token::Either(vec![Token::Char('\n'), Token::EOF]),
                Token::Char('a')
            )),
            s.read_eol()
        );
        assert_eq!(Ok(Rng::new(0, "a")), s.read_1());
        assert_eq!(Ok(Rng::new(1, "\n")), s.read_eol());
        assert_eq!(Ok(Rng::new(2, "\n")), s.read_eol());
        assert_eq!(Ok(Rng::new(3, "")), s.read_eol());
        assert_eq!(Ok(Rng::new(3, "")), s.read_eol());
    }

    #[test]
    fn test_read_space1() {
        let s = Scanner::new("  a\t\tb  \nc");

        assert_eq!(Ok(Rng::new(0, "  ")), s.read_space1());
        assert_eq!(Ok(Rng::new(2, "a")), s.read_1());
        assert_eq!(Ok(Rng::new(3, "\t\t")), s.read_space1());
        assert_eq!(
            Err(SyntaxError::new(
                s.source,
                5,
                None,
                Token::WhiteSpace,
                Token::Char('b')
            )),
            s.read_space1()
        );
        assert_eq!(Ok(Rng::new(5, "b")), s.read_1());
        assert_eq!(Ok(Rng::new(6, "  ")), s.read_space1());
        assert_eq!(Ok(Rng::new(8, "")), s.read_space1());
        assert_eq!(Ok(Rng::new(8, "\n")), s.read_eol());
        assert_eq!(Ok(Rng::new(9, "c")), s.read_1());
        assert_eq!(Ok(Rng::new(10, "")), s.read_eol());
    }
}
