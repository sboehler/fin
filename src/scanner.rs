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

pub struct Scanner<'a> {
    source: &'a str,
    filename: Option<PathBuf>,
    chars: Peekable<CharIndices<'a>>,
}

impl<'a> Scanner<'a> {
    pub fn new(s: &'a str, filename: Option<PathBuf>) -> Scanner<'a> {
        Scanner {
            source: &s,
            filename,
            chars: s.char_indices().peekable(),
        }
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

    pub fn read_while<P>(&mut self, pred: P) -> Result<&'a str>
    where
        P: Fn(char) -> bool,
    {
        let start = self.pos();
        self.skip_while(pred);
        let end = self.pos();
        Ok(&self.source[start..end])
    }

    pub fn read_until<P>(&mut self, pred: P) -> Result<&'a str>
    where
        P: Fn(char) -> bool,
    {
        let start = self.pos();
        self.skip_while(|v| !pred(v));
        let end = self.pos();
        Ok(&self.source[start..end])
    }

    pub fn read_all(&mut self) -> Result<&'a str> {
        let start = self.pos();
        self.skip_while(|_| true);
        Ok(&self.source[start..])
    }

    pub fn consume_char(&mut self, c: char) -> Result<()> {
        match self.next() {
            Some(d) if c == d => Ok(()),
            Some(d) => Err(self.error(None, Character::Char(c), Character::Char(d))),
            None => Err(self.error(None, Character::Char(c), Character::EOF)),
        }
    }

    pub fn consume_string(&mut self, str: &str) -> Result<()> {
        for c in str.chars() {
            self.consume_char(c)?;
        }
        Ok(())
    }

    pub fn read_quoted_string(&mut self) -> Result<&'a str> {
        self.consume_char('"')?;
        let res = self.read_while(|c| c != '"')?;
        self.consume_char('"')?;
        Ok(res)
    }

    pub fn read_identifier(&mut self) -> Result<&'a str> {
        let res = self.read_while(|c| c.is_alphanumeric())?;
        match res {
            "" => {
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

    pub fn read_1(&mut self) -> Result<char> {
        self.next()
            .ok_or_else(|| self.error(None, Character::Any, Character::EOF))
    }

    pub fn read_n(&mut self, n: usize) -> Result<&'a str> {
        let start = self.pos();
        for _ in 0..n {
            self.read_1()?;
        }
        Ok(&self.source[start..self.pos()])
    }

    pub fn consume_eol(&mut self) -> Result<()> {
        match self.next() {
            None => Ok(()),
            Some('\n') => Ok(()),
            Some(ch) => return Err(self.error(None, Character::Char('\n'), Character::Char(ch))),
        }
    }

    pub fn consume_space1(&mut self) -> Result<()> {
        match self.current() {
            Some(ch) if !ch.is_ascii_whitespace() => {
                return Err(self.error(None, Character::WhiteSpace, Character::Char(ch)))
            }
            _ => Ok(self.consume_space()),
        }
    }

    pub fn consume_space(&mut self) {
        self.skip_while(|c| c != '\n' && c.is_ascii_whitespace())
    }

    pub fn consume_rest_of_line(&mut self) -> Result<()> {
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
    fn test_read_while() -> Result<()> {
        let mut s = Scanner::new("asdf", None);
        assert_eq!(s.read_while(|c| c != 'f')?, "asd");
        Ok(())
    }

    #[test]
    fn test_consume_while() -> Result<()> {
        let mut s = Scanner::new("asdf", None);
        s.skip_while(|c| c != 'f');
        s.consume_char('f')?;
        assert!(s.current().is_none());
        Ok(())
    }

    #[test]
    fn test_consume_char() -> Result<()> {
        let bs = "asdf";
        let mut s = Scanner::new(bs, None);
        for cp in bs.chars() {
            s.consume_char(cp)?
        }
        Ok(())
    }

    #[test]
    fn test_consume_string() -> Result<()> {
        let tests = ["asdf"];
        for test in tests {
            let mut s = Scanner::new(test, None);
            s.consume_string(test)?
        }
        Ok(())
    }

    #[test]
    fn test_read_quoted_string() -> Result<()> {
        let tests = [
            ("\"\"", ""),
            ("\"A String \"", "A String "),
            ("\"a\"\"", "a"),
        ];
        for (test, expected) in tests {
            let mut s = Scanner::new(test, None);
            assert_eq!(s.read_quoted_string()?, expected);
        }
        let tests = ["\""];
        for test in tests {
            let mut s = Scanner::new(test, None);
            assert!(s.read_quoted_string().is_err());
        }
        Ok(())
    }

    #[test]
    fn test_read_identifier() -> Result<()> {
        let tests = [
            ("23asdf 3asdf", "23asdf"),
            ("foo bar", "foo"),
            ("Foo Bar", "Foo"),
        ];
        for (test, expected) in tests {
            let mut s = Scanner::new(test, None);
            assert_eq!(s.read_identifier()?, expected);
        }
        let tests = [" "];
        for test in tests {
            let mut s = Scanner::new(test, None);
            assert!(s.read_identifier().is_err())
        }
        Ok(())
    }

    #[test]
    fn test_read_string() -> Result<()> {
        let tests = [
            ("23asdf 3asdf", "23as", "df 3asdf"),
            ("foo bar", "foo ", "bar"),
            ("Foo Bar", "Foo ", "Bar"),
        ];
        for (test, expected, remainder) in tests {
            let mut s = Scanner::new(test, None);
            assert_eq!(s.read_n(4)?, expected);
            assert_eq!(s.read_all()?, remainder)
        }
        for (test, _, _) in tests {
            let mut s = Scanner::new(test, None);
            assert!(s.read_n(test.len() + 1).is_err());
            assert_eq!(s.read_all()?, "")
        }
        Ok(())
    }

    #[test]
    fn test_consume_eol() -> Result<()> {
        let tests = ["", "\n", "\na"];
        for test in tests {
            let mut s = Scanner::new(test, None);
            s.consume_eol()?
        }
        let tests = [" ", "not an eol", "na"];
        for test in tests {
            let mut s = Scanner::new(test, None);
            assert!(s.consume_eol().is_err())
        }
        Ok(())
    }

    #[test]
    fn test_consume_space1() -> Result<()> {
        let tests = ["", "\n", "\t"];
        for test in tests {
            let mut s = Scanner::new(test, None);
            s.consume_space1()?
        }
        let tests = ["a\n", "n", "na"];
        for test in tests {
            let mut s = Scanner::new(test, None);
            assert!(s.consume_space1().is_err())
        }
        Ok(())
    }
}
