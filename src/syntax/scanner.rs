use std::{cell::RefCell, iter::Peekable, path::PathBuf, str::CharIndices};

#[derive(Debug, Eq, PartialEq)]
pub struct ParserError {
    got: Token,
    want: Token,
    msg: Option<String>,
    file: Option<String>,
    line: usize,
    col: usize,
    context: Vec<(usize, String)>,
}

impl ParserError {
    pub fn new(
        source: &str,
        file: Option<&PathBuf>,
        pos: usize,
        msg: Option<String>,
        want: Token,
        got: Token,
    ) -> ParserError {
        let (line, col) = Self::position(source, pos);
        let rng = line.saturating_sub(4)..=line;
        let file = file.map(|p| p.to_string_lossy().to_string());
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
        }
    }

    fn position(t: &str, pos: usize) -> (usize, usize) {
        let lines: Vec<_> = t[..pos].split(|c| c == '\n').collect();
        let line = lines.len().saturating_sub(1);
        let col = lines.last().map(|s| s.len()).unwrap_or(0);
        (line, col)
    }

    pub fn update(mut self, msg: &str) -> Self {
        self.msg = Some(msg.into());
        self
    }
}

impl std::fmt::Display for ParserError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        writeln!(f)?;
        write!(
            f,
            "{file}:{line}:{col}:",
            file = self.file.as_deref().unwrap_or(""),
            line = self.line,
            col = self.col,
        )?;
        if let Some(ref s) = self.msg {
            writeln!(f, " while {}", s)?;
        } else {
            writeln!(f)?;
        }
        writeln!(f)?;
        for (n, line) in &self.context {
            writeln!(f, "{:5}|{}", n, line)?;
        }
        writeln!(
            f,
            "{}^ want {}, got {}",
            " ".repeat(self.col + 6),
            self.want,
            self.got
        )?;
        if let Token::Error(ref e) = self.got {
            e.fmt(f)?;
        }
        Ok(())
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
    Error(Box<ParserError>),
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
            Self::Error(_) => write!(f, "error"),
            Self::Char(ch) => write!(f, "'{}'", ch.escape_debug()),
            Self::Digit => write!(f, "a digit (0-9)"),
            Self::AlphaNum => {
                write!(f, "a character (a-z, A-Z) or a digit (0-9)")
            }
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

#[cfg(test)]
mod test_parser_error {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_read_while() {
        assert_eq!(
            vec![
                "",
                "finance.knut:0:1: while parsing file",
                "",
                "    0|asdf",
                "       ^ want whitespace, got a character (a-z, A-Z) or a digit (0-9)",
                ""
            ]
            .join("\n"),
            ParserError {
                got: Token::AlphaNum,
                want: Token::WhiteSpace,
                msg: Some("parsing file".into()),
                file: Some("finance.knut".into()),
                line: 0,
                col: 1,
                context: vec![(0, "asdf".into())]
            }.to_string()
        );
        assert_eq!(ParserError::position("foo\nbar\n", 0), (0, 0));
        assert_eq!(ParserError::position("foo\nbar\n", 1), (0, 1));
        assert_eq!(ParserError::position("foo\nbar\n", 2), (0, 2));
        assert_eq!(ParserError::position("foo\nbar\n", 3), (0, 3));
        assert_eq!(ParserError::position("foo\nbar\n", 4), (1, 0));
        assert_eq!(ParserError::position("foo\nbar\n", 5), (1, 1));
        assert_eq!(ParserError::position("foo\nbar\n", 6), (1, 2));
        assert_eq!(ParserError::position("foo\nbar\n", 7), (1, 3));
        assert_eq!(ParserError::position("foo\nbar\n", 8), (2, 0));
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct Range<'a> {
    pub start: usize,
    pub str: &'a str,
}

impl<'a> Range<'a> {
    pub fn new(start: usize, str: &'a str) -> Range<'a> {
        Range {
            start,
            str,
        }
    }

    pub fn len(&self) -> usize {
        self.str.len()
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
        Range::new(start, &self.source[start..self.pos()])
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
        match self.current() {
            Some(d) if c == d => {
                self.advance();
                Ok(self.range_from(start))
            }
            o => Err(self.error(
                self.pos(),
                None,
                Token::Char(c),
                Token::from_char(o),
            )),
        }
    }

    pub fn read_string(&self, str: &str) -> Result<Range> {
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
                Some("parsing identifier".into()),
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
        match self.current() {
            Some(c) if pred(c) => {
                self.advance();
                Ok(self.range_from(start))
            }
            Some(c) => Err(self.error(start, None, token, Token::Char(c))),
            None => Err(self.error(start, None, token, Token::EOF)),
        }
    }

    pub fn read_n_with<P>(
        &self,
        n: usize,
        token: Token,
        pred: P,
    ) -> Result<Range>
    where
        P: Fn(char) -> bool,
    {
        let start = self.pos();
        for _ in 0..n {
            match self.current() {
                Some(c) if pred(c) => self.advance(),
                Some(c) => {
                    return Err(self.error(
                        self.pos(),
                        None,
                        token,
                        Token::Char(c),
                    ))
                }
                None => {
                    return Err(self.error(self.pos(), None, token, Token::EOF))
                }
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
        match self.current() {
            None | Some('\n') => {
                self.advance();
                Ok(self.range_from(start))
            }
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

    fn error(
        &self,
        pos: usize,
        msg: Option<String>,
        want: Token,
        got: Token,
    ) -> ParserError {
        ParserError::new(
            self.source,
            self.filename.as_ref(),
            pos,
            msg,
            want,
            got,
        )
    }
}

#[cfg(test)]
mod test_scanner {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_read_while() {
        let s = Scanner::new("aaasdff");
        assert_eq!(Range::new(0, "aaasd".into()), s.read_while(|c| c != 'f'));
        assert_eq!(Range::new(5, "ff".into()), s.read_while(|c| c == 'f'));
        assert_eq!(Range::new(7, "".into()), s.read_while(|c| c == 'q'));
        assert_eq!(Ok(Range::new(7, "")), s.consume_eol());
    }

    #[test]
    fn test_read_char() {
        let s = Scanner::new("asdf".into());
        assert_eq!(Ok(Range::new(0, "a")), s.read_char('a'));
        assert_eq!(
            Err(ParserError::new(
                "asdf",
                None,
                1,
                None,
                Token::Char('q'),
                Token::Char('s')
            )),
            s.read_char('q')
        );
        assert_eq!(Ok(Range::new(1, "s")), s.read_char('s'));
        assert_eq!(Ok(Range::new(2, "d")), s.read_char('d'));
        assert_eq!(Ok(Range::new(3, "f")), s.read_char('f'));
        assert_eq!(Ok(Range::new(4, "")), s.consume_eol());
    }

    #[test]
    fn test_read_string() {
        let s = Scanner::new("asdf");

        assert_eq!(Ok(Range::new(0, "as")), s.read_string("as"),);
        assert_eq!(
            Err(ParserError::new(
                "asdf",
                None,
                2,
                None,
                Token::Char('q'),
                Token::Char('d')
            )),
            s.read_char('q')
        );
        assert_eq!(Ok(Range::new(2, "df")), s.read_string("df"));
        assert_eq!(Ok(Range::new(4, "")), s.consume_eol());
    }

    #[test]
    fn test_read_identifier() {
        let s = Scanner::new("foo bar 1baz");
        assert_eq!(Ok(Range::new(0, "foo")), s.read_identifier());
        assert_eq!(
            Range::new(3, " "),
            s.read_while(|c| c.is_ascii_whitespace())
        );
        assert_eq!(Ok(Range::new(4, "bar")), s.read_identifier());
        assert_eq!(
            Range::new(7, " "),
            s.read_while(|c| c.is_ascii_whitespace())
        );
        assert_eq!(Ok(Range::new(8, "1baz")), s.read_identifier());
        assert_eq!(Ok(Range::new(12, "")), s.consume_eol());
    }

    #[test]
    fn test_read_1() {
        let s = Scanner::new("foo");
        assert_eq!(Ok(Range::new(0, "f")), s.read_1());
        assert_eq!(Ok(Range::new(1, "o")), s.read_1());
        assert_eq!(Ok(Range::new(2, "o")), s.read_1());
        assert_eq!(
            Err(ParserError::new("foo", None, 3, None, Token::Any, Token::EOF)),
            s.read_1()
        );
        assert_eq!(Ok(Range::new(3, "")), s.consume_eol());
    }

    #[test]
    fn test_read_1_with() {
        let s = Scanner::new("asdf");
        assert_eq!(
            Ok(Range::new(0, "a")),
            s.read_1_with(Token::Char('a'), |c| c == 'a'),
        );
        assert_eq!(
            Ok(Range::new(1, "s")),
            s.read_1_with(Token::Custom("no a".into()), |c| c != 'a')
        );
        assert_eq!(
            Err(ParserError::new(
                "asdf",
                None,
                2,
                None,
                Token::Digit,
                Token::Char('d')
            )),
            s.read_1_with(Token::Digit, |c| c.is_ascii_digit())
        );
        assert_eq!(
            Ok(Range::new(2, "d")),
            s.read_1_with(Token::Char('d'), |c| c == 'd')
        );
        assert_eq!(
            Ok(Range::new(3, "f")),
            s.read_1_with(Token::Char('f'), |c| c == 'f')
        );
        assert_eq!(
            Err(ParserError::new(
                "asdf",
                None,
                4,
                None,
                Token::Any,
                Token::EOF
            )),
            s.read_1_with(Token::Any, |_| true)
        );
        assert_eq!(Ok(Range::new(4, "")), s.consume_eol());
    }

    #[test]
    fn test_read_n() {
        let s = Scanner::new("asdf");
        assert_eq!(
            Ok(Range {
                start: 0,
                str: "as".into()
            }),
            s.read_n(2)
        );
        assert_eq!(
            Ok(Range {
                start: 2,
                str: "".into()
            }),
            s.read_n(0)
        );
        assert_eq!(
            Err(ParserError::new(
                "asdf",
                None,
                4,
                None,
                Token::Any,
                Token::EOF
            )),
            s.read_n(3)
        );
        assert_eq!(Ok(Range::new(4, "")), s.consume_eol());
    }

    #[test]
    fn test_consume_eol() {
        let s = Scanner::new("a\n\n");
        assert_eq!(
            Err(ParserError::new(
                "a\n\n",
                None,
                0,
                None,
                Token::Either(vec![Token::Char('\n'), Token::EOF]),
                Token::Char('a')
            )),
            s.consume_eol()
        );
        assert_eq!(Ok(Range::new(0, "a")), s.read_1());
        assert_eq!(Ok(Range::new(1, "\n")), s.consume_eol());
        assert_eq!(Ok(Range::new(2, "\n")), s.consume_eol());
        assert_eq!(Ok(Range::new(3, "")), s.consume_eol());
        assert_eq!(Ok(Range::new(3, "")), s.consume_eol());
    }

    #[test]
    fn test_consume_space1() {
        let s = Scanner::new("  a\t\tb  \nc");

        assert_eq!(Ok(Range::new(0, "  ")), s.consume_space1());
        assert_eq!(Ok(Range::new(2, "a")), s.read_1());
        assert_eq!(Ok(Range::new(3, "\t\t")), s.consume_space1());
        assert_eq!(
            Err(ParserError::new(
                s.source,
                None,
                5,
                None,
                Token::WhiteSpace,
                Token::Char('b')
            )),
            s.consume_space1()
        );
        assert_eq!(Ok(Range::new(5, "b")), s.read_1());
        assert_eq!(Ok(Range::new(6, "  ")), s.consume_space1());
        assert_eq!(Ok(Range::new(8, "")), s.consume_space1());
        assert_eq!(Ok(Range::new(8, "\n")), s.consume_eol());
        assert_eq!(Ok(Range::new(9, "c")), s.read_1());
        assert_eq!(Ok(Range::new(10, "")), s.consume_eol());
    }
}
