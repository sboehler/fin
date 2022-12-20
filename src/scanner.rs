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
        writeln!(f, "")?;
        for (n, line) in &self.context {
            writeln!(f, "{:03}: {}", n, line)?;
        }
        writeln!(f, "     {}{}", "_".repeat(self.col), "^")?;
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
                for ch in chars {
                    write!(f, "{},", ch)?;
                }
                writeln!(f, "")?;
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
    positions: Vec<usize>,
}

impl<'a> Scanner<'a> {
    pub fn new_from_file(s: &'a str, filename: Option<PathBuf>) -> Scanner<'a> {
        Scanner {
            source: &s,
            filename,
            chars: s.char_indices().peekable(),
            positions: Vec::new(),
        }
    }

    pub fn new(s: &'a str) -> Scanner<'a> {
        Scanner::new_from_file(s, None)
    }

    pub fn annotate<T>(&mut self, t: T) -> Result<Annotated<T>> {
        Ok(Annotated(t, (self.positions.pop().unwrap(), self.pos())))
    }

    pub fn mark_position(&mut self) {
        let pos = self.pos();
        self.positions.push(pos)
    }

    pub fn current(&mut self) -> Option<char> {
        self.chars.peek().map(|t| t.1)
    }

    pub fn next(&mut self) -> Option<char> {
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
            self.next();
        }
    }

    pub fn read_while<P>(&mut self, pred: P) -> Result<Annotated<&'a str>>
    where
        P: Fn(char) -> bool,
    {
        self.mark_position();
        let start = self.pos();
        self.skip_while(pred);
        let end = self.pos();
        self.annotate(&self.source[start..end])
    }

    pub fn read_until<P>(&mut self, pred: P) -> Result<Annotated<&'a str>>
    where
        P: Fn(char) -> bool,
    {
        self.mark_position();
        let start = self.pos();
        self.skip_while(|v| !pred(v));
        let end = self.pos();
        self.annotate(&self.source[start..end])
    }

    pub fn read_all(&mut self) -> Result<Annotated<&'a str>> {
        self.mark_position();
        let start = self.pos();
        self.skip_while(|_| true);
        self.annotate(&self.source[start..])
    }

    pub fn consume_char(&mut self, c: char) -> Result<Annotated<()>> {
        self.mark_position();
        match self.next() {
            Some(d) if c == d => self.annotate(()),
            o => Err(self.error(None, Character::Char(c), Character::from_char(o))),
        }
    }

    pub fn consume_string(&mut self, str: &str) -> Result<Annotated<()>> {
        self.mark_position();
        for c in str.chars() {
            self.consume_char(c)?;
        }
        self.annotate(())
    }

    pub fn read_quoted_string(&mut self) -> Result<Annotated<&'a str>> {
        self.mark_position();
        self.consume_char('"')?;
        let res = self.read_while(|c| c != '"')?;
        self.consume_char('"')?;
        self.annotate(res.0)
    }

    pub fn read_identifier(&mut self) -> Result<Annotated<&'a str>> {
        let res = self.read_while(|c| c.is_alphanumeric())?;
        match res {
            Annotated("", _) => {
                let got = Character::from_char(self.current());
                Err(self.error(
                    Some("error while parsing identifier".into()),
                    Character::Any,
                    got,
                ))
            }
            _ => Ok(res),
        }
    }

    pub fn read_1(&mut self) -> Result<Annotated<char>> {
        self.mark_position();
        match self.next() {
            Some(c) => self.annotate(c),
            None => Err(self.error(None, Character::Any, Character::EOF)),
        }
    }

    pub fn read_n(&mut self, n: usize) -> Result<Annotated<&'a str>> {
        self.mark_position();
        let start = self.pos();
        for _ in 0..n {
            self.read_1()?;
        }
        let res = &self.source[start..self.pos()];
        self.annotate(res)
    }

    pub fn consume_eol(&mut self) -> Result<Annotated<()>> {
        self.mark_position();
        match self.next() {
            None | Some('\n') => self.annotate(()),
            Some(ch) => return Err(self.error(None, Character::Char('\n'), Character::Char(ch))),
        }
    }

    pub fn consume_space1(&mut self) -> Result<Annotated<()>> {
        self.mark_position();
        match self.current() {
            Some(ch) if !ch.is_ascii_whitespace() => {
                return Err(self.error(None, Character::WhiteSpace, Character::Char(ch)))
            }
            _ => {
                let res = self.consume_space();
                self.annotate(res)
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

    pub fn error(&mut self, msg: Option<String>, want: Character, got: Character) -> ParserError {
        let lines: Vec<_> = self.source[..self.pos()].lines().collect();
        let line = lines.len().checked_sub(1).unwrap_or(0);
        let col = lines.last().map(|s| s.len()).unwrap_or(0);
        let rng = lines.len().checked_sub(2).unwrap_or(0)..=lines.len();
        let file = self
            .filename
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or("<stream>".into());
        let context = self
            .source
            .lines()
            .enumerate()
            .filter(|t| rng.contains(&t.0))
            .map(|(i, l)| (i, l.into()))
            .collect();
        return ParserError {
            file,
            line,
            col,
            context,
            msg,
            want,
            got,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_while() {
        let mut s = Scanner::new("asdf");
        assert_eq!(
            s.read_while(|c| c != 'f').unwrap(),
            Annotated("asd", (0, 3))
        )
    }

    #[test]
    fn test_consume_while() {
        let mut s = Scanner::new("asdf");
        s.skip_while(|c| c != 'f');
        s.consume_char('f').unwrap();
        assert!(s.current().is_none());
    }

    #[test]
    fn test_consume_char() {
        let mut s = Scanner::new("asdf");
        for cp in "asdf".chars() {
            assert!(s.consume_char(cp).is_ok());
        }
    }

    #[test]
    fn test_consume_string() {
        assert!(Scanner::new("asdf").consume_string("asdf").is_ok())
    }

    #[test]
    fn read_quoted_string() {
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
        let tests = [
            ("23asdf 3asdf", "23asdf"),
            ("foo bar", "foo"),
            ("Foo Bar", "Foo"),
        ];
        for (test, expected) in tests {
            let mut s = Scanner::new(test);
            assert_eq!(
                s.read_identifier().unwrap(),
                Annotated(expected, (0, expected.len()))
            );
        }
        assert!(Scanner::new(" ").read_identifier().is_err())
    }

    #[test]
    fn test_read_n() {
        let mut s = Scanner::new("23asdflj");
        assert_eq!(s.read_n(4).unwrap(), Annotated("23as", (0, 4)));

        let tests = [
            ("23asdf 3asdf", "23as", "df 3asdf"),
            ("foo bar", "foo ", "bar"),
            ("Foo Bar", "Foo ", "Bar"),
        ];
        for (test, expected, remainder) in tests {
            let mut s = Scanner::new(test);
            assert_eq!(
                s.read_n(4).unwrap(),
                Annotated(expected, (0, expected.len()))
            );
            assert_eq!(
                s.read_all().unwrap(),
                Annotated(remainder, (expected.len(), test.len()))
            )
        }
        for (test, _, _) in tests {
            let mut s = Scanner::new(test);
            assert!(s.read_n(test.len() + 1).is_err());
            assert_eq!(
                s.read_all().unwrap(),
                Annotated("".into(), (test.len(), test.len()))
            )
        }
    }

    #[test]
    fn test_consume_eol() {
        let tests = ["", "\n", "\na"];
        for test in tests {
            let mut s = Scanner::new(test);
            assert!(s.consume_eol().is_ok());
        }
        let tests = [" ", "not an eol", "na"];
        for test in tests {
            let mut s = Scanner::new(test);
            assert!(s.consume_eol().is_err())
        }
    }

    #[test]
    fn test_consume_space1() {
        let tests = ["", "\n", "\t"];
        for test in tests {
            let mut s = Scanner::new(test);
            s.consume_space1().unwrap();
        }
        let tests = ["a\n", "n", "na"];
        for test in tests {
            let mut s = Scanner::new(test);
            assert!(s.consume_space1().is_err())
        }
    }
}
