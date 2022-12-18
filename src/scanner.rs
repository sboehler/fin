use std::{iter::Peekable, vec::IntoIter};

#[derive(Debug, Copy, Clone)]
pub struct Position {
    line: u64,
    column: u64,
}

impl Position {
    pub fn new() -> Self {
        Position { line: 0, column: 0 }
    }
    pub fn update(&mut self, cur: Option<char>, new: Option<char>) {
        match (cur, new) {
            (Some(c), _) if c != '\n' => self.column += 1,
            (_, Some(_)) => {
                self.line += 1;
                self.column = 1;
            }
            _ => {}
        }
    }
}

impl Default for Position {
    fn default() -> Self {
        Position::new()
    }
}

impl std::fmt::Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "(Line: {}, Column: {})", self.line, self.column)
    }
}

#[derive(Debug)]
pub enum ParserError {
    Unexpected(Position, String),
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

pub struct Scanner {
    chars: Peekable<IntoIter<char>>,
    pos: Position,
}

impl Scanner {
    pub fn new(s: String) -> Scanner {
        let c = s.chars().collect::<Vec<char>>().into_iter().peekable();
        Scanner {
            chars: c,
            pos: Position::new(),
        }
    }

    pub fn current(&mut self) -> Option<char> {
        self.chars.peek().copied()
    }

    pub fn advance(&mut self) -> Option<char> {
        let cur = self.current();
        self.chars.next();
        let next = self.current();
        self.pos.update(cur, next);
        next
    }

    pub fn position(&self) -> Position {
        self.pos
    }
}

pub fn read_while<P>(s: &mut Scanner, pred: P) -> Result<String>
where
    P: Fn(&char) -> bool,
{
    let mut b = String::new();
    while let Some(c) = s.current() {
        if pred(&c) {
            b.push(c)
        } else {
            break;
        }
        s.advance();
    }
    Ok(b)
}

pub fn read_all(s: &mut Scanner) -> Result<String> {
    let mut b = String::new();
    while let Some(c) = s.current() {
        b.push(c);
        s.advance();
    }
    Ok(b)
}

pub fn consume_while<P>(s: &mut Scanner, pred: P) -> Result<()>
where
    P: Fn(&char) -> bool,
{
    while let Some(c) = s.current() {
        if !pred(&c) {
            break;
        }
        s.advance();
    }
    Ok(())
}

pub fn consume_char(s: &mut Scanner, c: char) -> Result<()> {
    match s.current() {
        Some(d) if c == d => {
            s.advance();
            Ok(())
        }
        Some(d) => Err(ParserError::Unexpected(
            s.position(),
            format!("Expected {:?}, got {:?}", c, d),
        )),
        None => Err(ParserError::Unexpected(
            s.position(),
            format!("Expected {:?}, got EOF", c),
        )),
    }
}

pub fn consume_string(s: &mut Scanner, str: &str) -> Result<()> {
    for c in str.chars() {
        consume_char(s, c)?;
    }
    Ok(())
}

pub fn read_quoted_string(s: &mut Scanner) -> Result<String> {
    consume_char(s, '"')?;
    let res = read_while(s, |c| *c != '"')?;
    consume_char(s, '"')?;
    Ok(res)
}

pub fn read_identifier(s: &mut Scanner) -> Result<String> {
    let res = read_while(s, |c| c.is_alphanumeric())?;
    if res.is_empty() {
        Err(ParserError::Unexpected(
            s.position(),
            "Expected identifier, got nothing".to_string(),
        ))
    } else {
        Ok(res)
    }
}

pub fn read_string(s: &mut Scanner, n: usize) -> Result<String> {
    let mut res = String::with_capacity(n);
    for _ in 0..n {
        match s.current() {
            Some(d) => {
                res.push(d);
                s.advance();
            }
            None => {
                return Err(ParserError::Unexpected(
                    s.position(),
                    "Expected more input, got EOF".to_string(),
                ))
            }
        }
    }
    Ok(res)
}

pub fn consume_eol(s: &mut Scanner) -> Result<()> {
    if s.current().is_none() {
        Ok(())
    } else {
        consume_char(s, '\n')
    }
}

pub fn consume_space1(s: &mut Scanner) -> Result<()> {
    if let Some(c) = s.current() {
        if !c.is_ascii_whitespace() {
            return Err(ParserError::Unexpected(
                s.position(),
                format!("Expected white space, got {:?}", c),
            ));
        }
    }
    consume_space(s)
}

pub fn consume_space(s: &mut Scanner) -> Result<()> {
    consume_while(s, |c| *c != '\n' && c.is_ascii_whitespace())
}

pub fn consume_rest_of_line(s: &mut Scanner) -> Result<()> {
    consume_while(s, |c| *c != '\n')?;
    consume_eol(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_while() -> Result<()> {
        let mut s = Scanner::new("asdf".into());
        assert_eq!(read_while(&mut s, |c| *c != 'f')?, "asd");
        Ok(())
    }

    #[test]
    fn test_consume_while() -> Result<()> {
        let mut s = Scanner::new("asdf".into());
        consume_while(&mut s, |&c| c != 'f')?;
        consume_char(&mut s, 'f')?;
        assert!(s.current().is_none());
        Ok(())
    }

    #[test]
    fn test_consume_char() -> Result<()> {
        let bs = "asdf";
        let mut s = Scanner::new(bs.into());
        for cp in bs.chars() {
            consume_char(&mut s, cp)?
        }
        Ok(())
    }

    #[test]
    fn test_consume_string() -> Result<()> {
        let tests = ["asdf"];
        for test in tests {
            let mut s = Scanner::new(test.into());
            consume_string(&mut s, test)?
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
            let mut s = Scanner::new(test.into());
            assert_eq!(read_quoted_string(&mut s)?, *expected);
        }
        let tests = ["\""];
        for test in tests {
            let mut s = Scanner::new(test.into());
            assert!(read_quoted_string(&mut s).is_err());
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
            let mut s = Scanner::new(test.into());
            assert_eq!(read_identifier(&mut s)?, *expected);
        }
        let tests = [" "];
        for test in tests {
            let mut s = Scanner::new(test.into());
            assert!(read_identifier(&mut s).is_err())
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
            let mut s = Scanner::new(test.into());
            assert_eq!(read_string(&mut s, 4)?, expected);
            assert_eq!(read_all(&mut s)?.as_str(), remainder)
        }
        for (test, _, _) in tests {
            let mut s = Scanner::new(test.into());
            assert!(read_string(&mut s, test.len() + 1).is_err());
            assert_eq!(read_all(&mut s)?.as_str(), "")
        }
        Ok(())
    }

    #[test]
    fn test_consume_eol() -> Result<()> {
        let tests = ["", "\n", "\na"];
        for test in tests {
            let mut s = Scanner::new(test.into());
            consume_eol(&mut s)?
        }
        let tests = [" ", "not an eol", "na"];
        for test in tests {
            let mut s = Scanner::new(test.into());
            assert!(consume_eol(&mut s).is_err())
        }
        Ok(())
    }

    #[test]
    fn test_consume_space1() -> Result<()> {
        let tests = ["", "\n", "\t"];
        for test in tests {
            let mut s = Scanner::new(test.into());
            consume_space1(&mut s)?
        }
        let tests = ["a\n", "n", "na"];
        for test in tests {
            let mut s = Scanner::new(test.into());
            assert!(consume_space1(&mut s).is_err())
        }
        Ok(())
    }
}
