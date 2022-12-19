use std::{iter::Peekable, str::CharIndices};

#[derive(Debug)]
pub enum ParserError {
    Unexpected(usize, String),
}

impl std::fmt::Display for ParserError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ParserError::Unexpected(pos, msg) => write!(f, "{}: Error: {}", pos, msg),
        }
    }
}

impl std::error::Error for ParserError {}

pub type Result<T> = std::result::Result<T, ParserError>;

pub struct Scanner<'a> {
    source: &'a str,
    chars: Peekable<CharIndices<'a>>,
}

impl<'a> Scanner<'a> {
    pub fn new(s: &'a str) -> Scanner<'a> {
        Scanner {
            source: &s,
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

    pub fn read_all(&mut self) -> Result<&'a str> {
        let start = self.pos();
        self.skip_while(|_| true);
        Ok(&self.source[start..])
    }

    pub fn consume_char(&mut self, c: char) -> Result<()> {
        match self.next() {
            Some(d) if c == d => Ok(()),
            Some(d) => Err(ParserError::Unexpected(
                self.pos(),
                format!("Expected {:?}, got {:?}", c, d),
            )),
            None => Err(ParserError::Unexpected(
                self.pos(),
                format!("Expected {:?}, got EOF", c),
            )),
        }
    }

    pub fn consume_string(&mut self, str: &str) -> Result<()> {
        for c in str.chars() {
            println!("{}", c);
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
            "" => Err(ParserError::Unexpected(
                self.pos(),
                "Expected identifier, got nothing".to_string(),
            )),
            _ => Ok(res),
        }
    }

    pub fn read_1(&mut self) -> Result<char> {
        self.chars.next().map(|t| t.1).ok_or_else(|| {
            ParserError::Unexpected(self.pos(), "expected one character, got EOF".into())
        })
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
            Some(ch) => {
                return Err(ParserError::Unexpected(
                    self.pos(),
                    format!("expected EOL, got '{}'", ch),
                ))
            }
        }
    }

    pub fn consume_space1(&mut self) -> Result<()> {
        match self.current() {
            Some(ch) if !ch.is_ascii_whitespace() => {
                return Err(ParserError::Unexpected(
                    self.pos(),
                    format!("expected white space, got {:?}", ch),
                ))
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_while() -> Result<()> {
        let mut s = Scanner::new("asdf");
        assert_eq!(s.read_while(|c| c != 'f')?, "asd");
        Ok(())
    }

    #[test]
    fn test_consume_while() -> Result<()> {
        let mut s = Scanner::new("asdf");
        s.skip_while(|c| c != 'f');
        s.consume_char('f')?;
        assert!(s.current().is_none());
        Ok(())
    }

    #[test]
    fn test_consume_char() -> Result<()> {
        let bs = "asdf";
        let mut s = Scanner::new(bs);
        for cp in bs.chars() {
            s.consume_char(cp)?
        }
        Ok(())
    }

    #[test]
    fn test_consume_string() -> Result<()> {
        let tests = ["asdf"];
        for test in tests {
            let mut s = Scanner::new(test);
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
            let mut s = Scanner::new(test);
            assert_eq!(s.read_quoted_string()?, expected);
        }
        let tests = ["\""];
        for test in tests {
            let mut s = Scanner::new(test);
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
            let mut s = Scanner::new(test);
            assert_eq!(s.read_identifier()?, expected);
        }
        let tests = [" "];
        for test in tests {
            let mut s = Scanner::new(test);
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
            let mut s = Scanner::new(test);
            assert_eq!(s.read_n(4)?, expected);
            assert_eq!(s.read_all()?, remainder)
        }
        for (test, _, _) in tests {
            let mut s = Scanner::new(test);
            assert!(s.read_n(test.len() + 1).is_err());
            assert_eq!(s.read_all()?, "")
        }
        Ok(())
    }

    #[test]
    fn test_consume_eol() -> Result<()> {
        let tests = ["", "\n", "\na"];
        for test in tests {
            let mut s = Scanner::new(test);
            s.consume_eol()?
        }
        let tests = [" ", "not an eol", "na"];
        for test in tests {
            let mut s = Scanner::new(test);
            assert!(s.consume_eol().is_err())
        }
        Ok(())
    }

    #[test]
    fn test_consume_space1() -> Result<()> {
        let tests = ["", "\n", "\t"];
        for test in tests {
            let mut s = Scanner::new(test);
            s.consume_space1()?
        }
        let tests = ["a\n", "n", "na"];
        for test in tests {
            let mut s = Scanner::new(test);
            assert!(s.consume_space1().is_err())
        }
        Ok(())
    }
}
