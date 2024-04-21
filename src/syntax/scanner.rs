use std::{iter::Peekable, path::PathBuf, str::CharIndices};

#[derive(Debug)]
pub struct ParserError {
    got: Token,
    want: Token,
    msg: Option<String>,
    file: String,
    line: usize,
    col: usize,
    context: Vec<(usize, String)>,
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
        Ok(())
    }
}

impl std::error::Error for ParserError {}

pub type Result<T> = std::result::Result<T, ParserError>;

#[derive(Debug, Eq, PartialEq)]
pub enum Token {
    EOF,
    Char(char),
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
    source: &'a str,
    filename: Option<PathBuf>,
    chars: Peekable<CharIndices<'a>>,
}

impl<'a> Scanner<'a> {
    pub fn new_from_file(s: &'a str, filename: Option<PathBuf>) -> Scanner<'a> {
        Scanner {
            source: s,
            filename,
            chars: s.char_indices().peekable(),
        }
    }

    pub fn new(s: &'a str) -> Scanner<'a> {
        Scanner::new_from_file(s, None)
    }

    pub fn range_from(&mut self, start: usize) -> Range {
        Range::new(start, self.pos(), &self.source[start..self.pos()])
    }

    pub fn current(&mut self) -> Option<char> {
        self.chars.peek().map(|t| t.1)
    }

    pub fn advance(&mut self) -> Option<char> {
        self.chars.next().map(|t| t.1)
    }

    pub fn pos(&mut self) -> usize {
        self.chars
            .peek()
            .map(|t| t.0)
            .unwrap_or_else(|| self.source.as_bytes().len())
    }

    pub fn read_while<P>(&mut self, pred: P) -> Range
    where
        P: Fn(char) -> bool,
    {
        let start = self.pos();
        while self.current().map(&pred).unwrap_or(false) {
            self.advance();
        }
        self.range_from(start)
    }

    pub fn read_until<P>(&mut self, pred: P) -> Range
    where
        P: Fn(char) -> bool,
    {
        self.read_while(|v| !pred(v))
    }

    pub fn read_all(&mut self) -> Range {
        self.read_while(|_| true)
    }

    pub fn consume_char(&mut self, c: char) -> Result<Range> {
        let start = self.pos();
        match self.advance() {
            Some(d) if c == d => Ok(self.range_from(start)),
            o => Err(self.error(start, None, Token::Char(c), Token::from_char(o))),
        }
    }

    pub fn consume_string(&mut self, str: &str) -> Result<Range> {
        let start = self.pos();
        for c in str.chars() {
            self.consume_char(c)?;
        }
        Ok(self.range_from(start))
    }

    pub fn read_identifier(&mut self) -> Result<Range> {
        let start = self.pos();
        let ident = self.read_while(|c| c.is_alphanumeric());
        if ident.len() == 0 {
            let got = Token::from_char(self.current());
            Err(self.error(
                start,
                Some("error while parsing identifier".into()),
                Token::Custom("alphanumeric character to start the identifier".into()),
                got,
            ))
        } else {
            Ok(self.range_from(start))
        }
    }

    pub fn read_1(&mut self) -> Result<Range> {
        let start = self.pos();
        match self.advance() {
            Some(_) => Ok(self.range_from(start)),
            None => Err(self.error(start, None, Token::Any, Token::EOF)),
        }
    }

    pub fn read_n(&mut self, n: usize) -> Result<Range> {
        let start = self.pos();
        for _ in 0..n {
            self.read_1()?;
        }
        Ok(self.range_from(start))
    }

    pub fn consume_eol(&mut self) -> Result<Range> {
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

    pub fn consume_space1(&mut self) -> Result<Range> {
        let start = self.pos();
        match self.current() {
            Some(ch) if !ch.is_ascii_whitespace() => {
                Err(self.error(start, None, Token::WhiteSpace, Token::Char(ch)))
            }
            _ => Ok(self.consume_space()),
        }
    }

    pub fn consume_space(&mut self) -> Range {
        self.read_while(|c| c != '\n' && c.is_ascii_whitespace())
    }

    pub fn consume_rest_of_line(&mut self) -> Result<Range> {
        let start = self.pos();
        self.read_while(|c| c != '\n');
        self.consume_eol()?;
        Ok(self.range_from(start))
    }

    pub fn error(
        &mut self,
        pos: usize,
        msg: Option<String>,
        want: Token,
        got: Token,
    ) -> ParserError {
        let lines: Vec<_> = self.source[..pos + 1].lines().collect();
        let line = lines.len().saturating_sub(1);
        let col = lines.last().map(|s| s.len().saturating_sub(1)).unwrap_or(0);
        let rng = lines.len().saturating_sub(5)..=lines.len().saturating_sub(1);
        let file = self
            .filename
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "<stream>".into());
        let context = self
            .source
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
        let mut s = Scanner::new("asdf");
        s.read_while(|c| c != 'f');
        assert_eq!(s.consume_char('f').unwrap().str, "f");
        assert!(s.current().is_none());
    }

    #[test]
    fn test_consume_char() {
        let t = "asdf";
        let mut s = Scanner::new(t);
        assert_eq!(s.consume_char('a').unwrap().str, "a");
        assert_eq!(s.consume_char('s').unwrap().str, "s");
        assert_eq!(s.consume_char('d').unwrap().str, "d");
        assert_eq!(s.consume_char('f').unwrap().str, "f");
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
    fn test_read_n() {
        assert_eq!(Scanner::new("23asdflj").read_n(4).unwrap().str, "23as");
        assert_eq!(Scanner::new("foo bar").read_n(4).unwrap().str, "foo ");
        assert_eq!(Scanner::new("foo").read_n(3).unwrap().str, "foo");
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
