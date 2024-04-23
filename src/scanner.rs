use std::{iter::Peekable, path::PathBuf, str::CharIndices};

#[derive(Debug)]
pub struct ParserError {
    got: Character,
    want: Character,
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
pub enum Character {
    EOF,
    Char(char),
    Either(Vec<Character>),
    Any,
    WhiteSpace,
    Custom(String),
}

impl Character {
    pub fn from_char(ch: Option<char>) -> Self {
        match ch {
            None => Self::EOF,
            Some(c) if c.is_whitespace() => Self::WhiteSpace,
            Some(c) => Self::Char(c),
        }
    }
}

impl std::fmt::Display for Character {
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
pub struct Annotated<T>(pub T, pub (usize, usize));

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

    pub fn annotate<T>(&mut self, prev: usize, t: T) -> Result<Annotated<T>> {
        Ok(Annotated(t, (prev, self.pos())))
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

    pub fn skip_while<P>(&mut self, pred: P)
    where
        P: Fn(char) -> bool,
    {
        while self.current().map(&pred).unwrap_or(false) {
            self.advance();
        }
    }

    pub fn read_while<P>(&mut self, pred: P) -> Annotated<&'a str>
    where
        P: Fn(char) -> bool,
    {
        let pos = self.pos();
        let start = self.pos();
        self.skip_while(pred);
        let end = self.pos();
        Annotated(&self.source[start..end], (pos, self.pos()))
    }

    pub fn read_until<P>(&mut self, pred: P) -> Annotated<&'a str>
    where
        P: Fn(char) -> bool,
    {
        let pos = self.pos();
        let start = self.pos();
        self.skip_while(|v| !pred(v));
        let end = self.pos();
        Annotated(&self.source[start..end], (pos, self.pos()))
    }

    pub fn read_all(&mut self) -> Annotated<&'a str> {
        let pos = self.pos();
        let start = self.pos();
        self.skip_while(|_| true);
        Annotated(&self.source[start..], (pos, self.pos()))
    }

    pub fn consume_char(&mut self, c: char) -> Result<Annotated<()>> {
        let pos = self.pos();
        match self.advance() {
            Some(d) if c == d => self.annotate(pos, ()),
            o => Err(self.error(
                pos,
                None,
                Character::Char(c),
                Character::from_char(o),
            )),
        }
    }

    pub fn consume_string(&mut self, str: &str) -> Result<Annotated<()>> {
        let pos = self.pos();
        for c in str.chars() {
            self.consume_char(c)?;
        }
        self.annotate(pos, ())
    }

    pub fn read_quoted_string(&mut self) -> Result<Annotated<&'a str>> {
        let pos = self.pos();
        self.consume_char('"')?;
        let res = self.read_while(|c| c != '"');
        self.consume_char('"')?;
        self.annotate(pos, res.0)
    }

    pub fn read_identifier(&mut self) -> Result<Annotated<&'a str>> {
        let pos = self.pos();
        let res = self.read_while(|c| c.is_alphanumeric()).0;
        match res {
            "" => {
                let got = Character::from_char(self.current());
                Err(self.error(
                    pos,
                    Some("error while parsing identifier".into()),
                    Character::Custom(
                        "alphanumeric character to start the identifier".into(),
                    ),
                    got,
                ))
            }
            _ => self.annotate(pos, res),
        }
    }

    pub fn read_1(&mut self) -> Result<Annotated<char>> {
        let pos = self.pos();
        match self.advance() {
            Some(c) => self.annotate(pos, c),
            None => Err(self.error(pos, None, Character::Any, Character::EOF)),
        }
    }

    pub fn read_n(&mut self, n: usize) -> Result<Annotated<&'a str>> {
        let pos = self.pos();
        let start = self.pos();
        for _ in 0..n {
            self.read_1()?;
        }
        let res = &self.source[start..self.pos()];
        self.annotate(pos, res)
    }

    pub fn consume_eol(&mut self) -> Result<Annotated<()>> {
        let pos = self.pos();
        match self.advance() {
            None | Some('\n') => self.annotate(pos, ()),
            Some(ch) => Err(self.error(
                pos,
                None,
                Character::Char('\n'),
                Character::Char(ch),
            )),
        }
    }

    pub fn consume_space1(&mut self) -> Result<Annotated<()>> {
        let pos = self.pos();
        match self.current() {
            Some(ch) if !ch.is_ascii_whitespace() => Err(self.error(
                pos,
                None,
                Character::WhiteSpace,
                Character::Char(ch),
            )),
            _ => {
                self.consume_space();
                self.annotate(pos, ())
            }
        }
    }

    pub fn consume_space(&mut self) {
        self.skip_while(|c| c != '\n' && c.is_ascii_whitespace())
    }

    pub fn consume_rest_of_line(&mut self) -> Result<Annotated<()>> {
        self.skip_while(|c| c != '\n');
        self.consume_eol()
    }

    pub fn error(
        &mut self,
        pos: usize,
        msg: Option<String>,
        want: Character,
        got: Character,
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
        assert_eq!(
            Scanner::new("asdf").read_while(|c| c != 'f'),
            Annotated("asd", (0, 3))
        )
    }

    #[test]
    fn test_consume_while() {
        let mut s = Scanner::new("asdf");
        s.skip_while(|c| c != 'f');
        assert_eq!(s.consume_char('f').unwrap(), Annotated((), (3, 4)));
        assert!(s.current().is_none());
    }

    #[test]
    fn test_consume_char() {
        let mut s = Scanner::new("asdf");
        assert_eq!(s.consume_char('a').unwrap(), Annotated((), (0, 1)));
        assert_eq!(s.consume_char('s').unwrap(), Annotated((), (1, 2)));
        assert_eq!(s.consume_char('d').unwrap(), Annotated((), (2, 3)));
        assert_eq!(s.consume_char('f').unwrap(), Annotated((), (3, 4)));
    }

    #[test]
    fn test_consume_string() {
        assert_eq!(
            Scanner::new("asdf").consume_string("asdf").unwrap(),
            Annotated((), (0, 4))
        )
    }

    #[test]
    fn test_read_quoted_string() {
        assert_eq!(
            Scanner::new("\"\"").read_quoted_string().unwrap(),
            Annotated("", (0, 2))
        );
        assert_eq!(
            Scanner::new("\"A String \"").read_quoted_string().unwrap(),
            Annotated("A String ", (0, 11))
        );
        assert_eq!(
            Scanner::new("\"a\"\"").read_quoted_string().unwrap(),
            Annotated("a", (0, 3))
        );
    }

    #[test]
    fn test_read_identifier() {
        assert_eq!(
            Scanner::new("23asdf 3asdf").read_identifier().unwrap(),
            Annotated("23asdf", (0, 6))
        );
        assert_eq!(
            Scanner::new("foo# bar").read_identifier().unwrap(),
            Annotated("foo", (0, 3))
        );
        assert_eq!(
            Scanner::new("Foo( Bar").read_identifier().unwrap(),
            Annotated("Foo", (0, 3))
        );
    }

    #[test]
    fn test_read_n() {
        assert_eq!(
            Scanner::new("23asdflj").read_n(4).unwrap(),
            Annotated("23as", (0, 4))
        );
        assert_eq!(
            Scanner::new("foo bar").read_n(4).unwrap(),
            Annotated("foo ", (0, 4))
        );
        assert_eq!(
            Scanner::new("foo").read_n(3).unwrap(),
            Annotated("foo", (0, 3))
        );
    }

    #[test]
    fn test_consume_eol() {
        assert_eq!(
            Scanner::new("").consume_eol().unwrap(),
            Annotated((), (0, 0))
        );
        assert_eq!(
            Scanner::new("\n").consume_eol().unwrap(),
            Annotated((), (0, 1))
        );
        assert_eq!(
            Scanner::new("\na").consume_eol().unwrap(),
            Annotated((), (0, 1))
        );
        assert!(Scanner::new(" ").consume_eol().is_err());
        assert!(Scanner::new("not eol").consume_eol().is_err())
    }

    #[test]
    fn test_consume_space1() {
        assert_eq!(
            Scanner::new("").consume_space1().unwrap(),
            Annotated((), (0, 0))
        );
        assert_eq!(
            Scanner::new("\n").consume_space1().unwrap(),
            Annotated((), (0, 0))
        );
        assert_eq!(
            Scanner::new("\n\n").consume_space1().unwrap(),
            Annotated((), (0, 0))
        );
        assert_eq!(
            Scanner::new("\t").consume_space1().unwrap(),
            Annotated((), (0, 1))
        );
        assert!(Scanner::new("a\n").consume_space1().is_err());
        assert!(Scanner::new("n").consume_space1().is_err());
        assert!(Scanner::new("na").consume_space1().is_err());
    }
}
