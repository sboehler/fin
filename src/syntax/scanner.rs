use std::{cell::RefCell, iter::Peekable, path::PathBuf, str::CharIndices};

#[derive(Debug, Eq, PartialEq)]
pub struct ParserError {
    got: Token,
    want: Token,
    msg: Option<String>,
    file: String,
    line: usize,
    col: usize,
    context: Vec<(usize, String)>,
    wrapped: Option<Box<ParserError>>,
}

impl ParserError {
    pub fn new(
        source: &str,
        file: &Option<PathBuf>,
        pos: usize,
        msg: Option<String>,
        want: Token,
        got: Token,
        wrapped: Option<ParserError>,
    ) -> ParserError {
        let lines: Vec<_> = source[..pos].lines().collect();
        let line = lines.len().saturating_sub(1);
        let col = lines.last().map(|s| s.len().saturating_sub(1)).unwrap_or(0);
        let rng = lines.len().saturating_sub(5)..=line;
        let file = file
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "<stream>".into());
        let context = source
            .lines()
            .enumerate()
            .filter(|t| rng.contains(&t.0))
            .map(|(i, l)| (i, l.into()))
            .collect();
        ParserError {
            file,
            line,
            col,
            context,
            msg,
            want,
            got,
            wrapped: wrapped.map(Box::new),
        }
    }
}

impl std::fmt::Display for ParserError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        writeln!(f)?;
        write!(
            f,
            "{file}:{line}:{col}:",
            file = self.file,
            line = self.line,
            col = self.col,
        )?;
        if let Some(ref s) = self.msg {
            writeln!(f, " {}", s)?;
        } else {
            writeln!(f)?;
        }
        writeln!(f, "-> got:  {}", self.got)?;
        writeln!(f, "-> want: {}", self.want)?;
        writeln!(f)?;
        for (n, line) in &self.context {
            writeln!(f, "{:5}:  {}", n, line)?;
        }
        writeln!(f, "{}^^^ want: {}", " ".repeat(self.col + 8), self.want)?;
        match &self.wrapped {
            Some(e) => e.fmt(f),
            _ => Ok(()),
        }
    }
}

impl std::error::Error for ParserError {}

pub type Result<T> = std::result::Result<T, ParserError>;

#[derive(Debug, Eq, PartialEq)]
pub enum Token {
    EOF,
    Char(char),
    Digit,
    AlphaNum,
    Either(Vec<Token>),
    Any,
    WhiteSpace,
    Custom(String),
}

impl Token {
    pub fn from_char(ch: Option<char>) -> Self {
        match ch {
            None => Self::EOF,
            Some(c) if c.is_whitespace() => Self::WhiteSpace,
            Some(c) => Self::Char(c),
        }
    }
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::EOF => write!(f, "EOF"),
            Self::Char(ch) => write!(f, "'{}'", ch.escape_debug()),
            Self::Digit => write!(f, "a digit (0-9)"),
            Self::AlphaNum => write!(f, "a character (a-z, A-Z) or a digit (0-9)"),
            Self::Any => write!(f, "any character"),
            Self::WhiteSpace => write!(f, "whitespace"),
            Self::Custom(s) => write!(f, "{}", s),
            Self::Either(chars) => {
                for (i, ch) in chars.iter().enumerate() {
                    write!(f, "{}", ch)?;
                    if i < chars.len().saturating_sub(1) {
                        write!(f, ", ")?;
                    }
                }
                writeln!(f)?;
                Ok(())
            }
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct Range<'a> {
    pub str: &'a str,
    pub start: usize,
    pub end: usize,
}

impl<'a> Range<'a> {
    pub fn new(start: usize, end: usize, str: &'a str) -> Range<'a> {
        Range { start, end, str }
    }

    pub fn len(&self) -> usize {
        self.end - self.start
    }
}

pub struct Scanner<'a> {
    pub source: &'a str,
    pub filename: Option<PathBuf>,
    chars: RefCell<Peekable<CharIndices<'a>>>,
}

impl<'a> Scanner<'a> {
    pub fn new_from_file(s: &'a str, filename: Option<PathBuf>) -> Scanner<'a> {
        Scanner {
            source: s,
            filename,
            chars: RefCell::new(s.char_indices().peekable()),
        }
    }

    pub fn new(s: &'a str) -> Scanner<'a> {
        Scanner::new_from_file(s, None)
    }

    pub fn range_from(&self, start: usize) -> Range {
        Range::new(start, self.pos(), &self.source[start..self.pos()])
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
            .map(|t| t.0)
            .unwrap_or_else(|| self.source.as_bytes().len())
    }

    pub fn read_while<P>(&self, pred: P) -> Range
    where
        P: Fn(char) -> bool,
    {
        let start = self.pos();
        while self.current().map(&pred).unwrap_or(false) {
            self.advance();
        }
        self.range_from(start)
    }

    pub fn read_until<P>(&self, pred: P) -> Range
    where
        P: Fn(char) -> bool,
    {
        self.read_while(|v| !pred(v))
    }

    pub fn read_all(&self) -> Range {
        self.read_while(|_| true)
    }

    pub fn read_char(&self, c: char) -> Result<Range> {
        let start = self.pos();
        match self.advance() {
            Some(d) if c == d => Ok(self.range_from(start)),
            o => Err(self.error(start, None, Token::Char(c), Token::from_char(o))),
        }
    }

    pub fn consume_string(&self, str: &str) -> Result<Range> {
        let start = self.pos();
        for c in str.chars() {
            self.read_char(c)?;
        }
        Ok(self.range_from(start))
    }

    pub fn read_identifier(&self) -> Result<Range> {
        let start = self.pos();
        let ident = self.read_while(|c| c.is_alphanumeric());
        if ident.len() == 0 {
            let got = Token::from_char(self.current());
            Err(self.error(
                start,
                Some("error while parsing identifier".into()),
                Token::AlphaNum,
                got,
            ))
        } else {
            Ok(self.range_from(start))
        }
    }

    pub fn read_1(&self) -> Result<Range> {
        let start = self.pos();
        match self.advance() {
            Some(_) => Ok(self.range_from(start)),
            None => Err(self.error(start, None, Token::Any, Token::EOF)),
        }
    }

    pub fn read_1_with<P>(&self, token: Token, pred: P) -> Result<Range>
    where
        P: Fn(char) -> bool,
    {
        let start = self.pos();
        match self.advance() {
            Some(c) if pred(c) => Ok(self.range_from(start)),
            Some(c) => Err(self.error(start, None, token, Token::Char(c))),
            None => Err(self.error(start, None, token, Token::EOF)),
        }
    }

    pub fn read_n_with<P>(&self, n: usize, token: Token, pred: P) -> Result<Range>
    where
        P: Fn(char) -> bool,
    {
        let start = self.pos();
        for _ in 0..n {
            match self.advance() {
                Some(c) if pred(c) => continue,
                Some(c) => return Err(self.error(start, None, token, Token::Char(c))),
                None => return Err(self.error(start, None, token, Token::EOF)),
            };
        }
        Ok(self.range_from(start))
    }

    pub fn read_n(&self, n: usize) -> Result<Range> {
        let start = self.pos();
        for _ in 0..n {
            self.read_1()?;
        }
        Ok(self.range_from(start))
    }

    pub fn consume_eol(&self) -> Result<Range> {
        let start = self.pos();
        match self.advance() {
            None | Some('\n') => Ok(self.range_from(start)),
            Some(ch) => Err(self.error(
                start,
                None,
                Token::Either(vec![Token::Char('\n'), Token::EOF]),
                Token::Char(ch),
            )),
        }
    }

    pub fn consume_space1(&self) -> Result<Range> {
        let start = self.pos();
        match self.current() {
            Some(ch) if !ch.is_ascii_whitespace() => {
                Err(self.error(start, None, Token::WhiteSpace, Token::Char(ch)))
            }
            _ => Ok(self.consume_space()),
        }
    }

    pub fn consume_space(&self) -> Range {
        self.read_while(|c| c != '\n' && c.is_ascii_whitespace())
    }

    pub fn consume_rest_of_line(&self) -> Result<Range> {
        let start = self.pos();
        self.read_while(|c| c != '\n');
        self.consume_eol()?;
        Ok(self.range_from(start))
    }

    fn error(&self, pos: usize, msg: Option<String>, want: Token, got: Token) -> ParserError {
        ParserError::new(&self.source, &self.filename, pos, msg, want, got, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_while() {
        assert_eq!(Scanner::new("asdf").read_while(|c| c != 'f').str, "asd")
    }

    #[test]
    fn test_consume_while() {
        let s = Scanner::new("asdf");
        s.read_while(|c| c != 'f');
        assert_eq!(s.read_char('f').unwrap().str, "f");
        assert!(s.current().is_none());
    }

    #[test]
    fn test_consume_char() {
        let t = "asdf";
        let s = Scanner::new(t);
        assert_eq!(s.read_char('a').unwrap().str, "a");
        assert_eq!(s.read_char('s').unwrap().str, "s");
        assert_eq!(s.read_char('d').unwrap().str, "d");
        assert_eq!(s.read_char('f').unwrap().str, "f");
    }

    #[test]
    fn test_consume_string() {
        assert_eq!(
            Scanner::new("asdf").consume_string("asdf").unwrap().str,
            "asdf"
        );
        assert_eq!(Scanner::new("asdf").consume_string("as").unwrap().str, "as")
    }

    #[test]
    fn test_read_identifier() {
        assert_eq!(
            Scanner::new("23asdf 3asdf").read_identifier().unwrap().str,
            "23asdf"
        );
        assert_eq!(
            Scanner::new("foo# bar").read_identifier().unwrap().str,
            "foo"
        );
        assert_eq!(
            Scanner::new("Foo( Bar").read_identifier().unwrap().str,
            "Foo"
        );
    }

    #[test]
    fn test_read_1() {
        assert_eq!(Scanner::new("23asdflj").read_1().unwrap().str, "2");
        assert_eq!(Scanner::new("foo bar").read_1().unwrap().str, "f");
        assert_eq!(Scanner::new("foo").read_1().unwrap().str, "f");
        assert!(Scanner::new("").read_1().is_err());
    }

    #[test]
    fn test_read_1_with() {
        assert_eq!(
            Scanner::new("23asdflj")
                .read_1_with(Token::Digit, |c| c.is_ascii_digit())
                .unwrap()
                .str,
            "2"
        );
        assert_eq!(
            Scanner::new("0foo bar")
                .read_1_with(Token::Digit, |c| c.is_ascii_digit())
                .unwrap()
                .str,
            "0"
        );
        assert!(Scanner::new("")
            .read_1_with(Token::Digit, |c| c.is_ascii_digit())
            .is_err());
        assert!(Scanner::new("a")
            .read_1_with(Token::Digit, |c| c.is_ascii_digit())
            .is_err());
    }

    #[test]
    fn test_read_n() {
        assert_eq!(Scanner::new("23asdflj").read_n(4).unwrap().str, "23as");
        assert_eq!(Scanner::new("foo bar").read_n(4).unwrap().str, "foo ");
        assert_eq!(Scanner::new("foo").read_n(3).unwrap().str, "foo");
        assert!(Scanner::new("foo").read_n(4).is_err());
    }

    #[test]
    fn test_consume_eol() {
        assert_eq!(Scanner::new("").consume_eol().unwrap().str, "");
        assert_eq!(Scanner::new("\n").consume_eol().unwrap().str, "\n");
        assert_eq!(Scanner::new("\na").consume_eol().unwrap().str, "\n");
        assert!(Scanner::new(" ").consume_eol().is_err());
        assert!(Scanner::new("not eol").consume_eol().is_err())
    }

    #[test]
    fn test_consume_space1() {
        assert_eq!(Scanner::new("").consume_space1().unwrap().str, "");
        assert_eq!(Scanner::new("\n").consume_space1().unwrap().str, "");
        assert_eq!(Scanner::new("\n\n").consume_space1().unwrap().str, "");
        assert_eq!(Scanner::new("\t").consume_space1().unwrap().str, "\t");
        assert!(Scanner::new("a\n").consume_space1().is_err());
        assert!(Scanner::new("n").consume_space1().is_err());
        assert!(Scanner::new("na").consume_space1().is_err());
    }
}
