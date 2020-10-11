use std::io::Bytes;
use std::io::Read;
use unicode_reader::CodePoints;

pub struct Scanner<R: Read> {
    codepoints: CodePoints<Bytes<R>>,
    cur: Option<char>,
    pos: Position,
}

#[derive(Debug, Copy, Clone)]
pub struct Position {
    line: u64,
    column: u64,
}

impl Position {
    fn update(&mut self, cur: Option<char>, new: Option<char>) {
        match new {
            Some('\n') => {
                self.line += 1;
                self.column = 0;
            }
            Some(_) => {
                self.column += 1;
            }
            None if cur.is_some() => {
                self.column += 1;
            }
            _ => {}
        };
    }
}

#[derive(Debug)]
pub enum ParserError {
    IO(Position, std::io::Error),
    Unexpected(Position, String),
}

impl std::fmt::Display for ParserError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ParserError::Unexpected(pos, msg) => write!(f, "{}: {}", pos, msg),
            ParserError::IO(pos, err) => {
                write!(f, "{}: IO Error: ", pos)?;
                err.fmt(f)
            }
        }
    }
}

impl std::error::Error for ParserError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ParserError::IO(_, err) => Some(err),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, ParserError>;

impl std::fmt::Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "(Line: {}, Column: {})", self.line, self.column)
    }
}

impl<R: Read> Scanner<R> {
    pub fn new(r: R) -> Scanner<R> {
        Scanner {
            codepoints: CodePoints::from(r),
            cur: None,
            pos: Position { line: 0, column: 0 },
        }
    }
    pub fn current(&self) -> Option<char> {
        self.cur
    }
    pub fn advance(&mut self) -> Result<()> {
        let next = self
            .codepoints
            .next()
            .transpose()
            .map_err(|e| ParserError::IO(self.position(), e))?;
        self.pos.update(self.cur, next);
        self.cur = next;
        Ok(())
    }

    pub fn position(&self) -> Position {
        return self.pos;
    }
}

// impl<R: Read> Iterator for Scanner<R> {
//     type Item = Result<char>;
//     fn next(&mut self) -> Option<Result<char>> {
//         match self.advance() {
//             Err(e) => Some(Err(e)),
//             Ok(_) => self.current().map(|c| Ok(c)),
//         }
//     }
// }

pub fn read_while<R: Read, P>(s: &mut Scanner<R>, pred: P) -> Result<String>
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
        s.advance()?
    }
    Ok(b)
}

pub fn read_all<R: Read>(s: &mut Scanner<R>) -> Result<String> {
    let mut b = String::new();
    while let Some(c) = s.current() {
        b.push(c);
        s.advance()?
    }
    Ok(b)
}

pub fn consume_while<R: Read, P>(s: &mut Scanner<R>, pred: P) -> Result<()>
where
    P: Fn(&char) -> bool,
{
    while let Some(c) = s.current() {
        if !pred(&c) {
            break;
        }
        s.advance()?
    }
    Ok(())
}

pub fn consume_char<R: Read>(s: &mut Scanner<R>, c: char) -> Result<()> {
    match s.current() {
        Some(d) => {
            if c == d {
                s.advance()
            } else {
                Err(ParserError::Unexpected(
                    s.position(),
                    format!("Expected '{}', got '{}'", c, d),
                ))
            }
        }
        None => Err(ParserError::Unexpected(
            s.position(),
            format!("Expected '{}', got EOF", c),
        )),
    }
}

pub fn consume_string<R: Read>(s: &mut Scanner<R>, str: &str) -> Result<()> {
    for c in str.chars() {
        consume_char(s, c)?;
    }
    Ok(())
}

pub fn read_quoted_string<R: Read>(s: &mut Scanner<R>) -> Result<String> {
    consume_char(s, '"')?;
    let res = read_while(s, |c| *c != '"')?;
    consume_char(s, '"')?;
    Ok(res)
}

pub fn read_identifier<R: Read>(s: &mut Scanner<R>) -> Result<String> {
    let res = read_while(s, |c| c.is_alphanumeric())?;
    if res.len() == 0 {
        Err(ParserError::Unexpected(
            s.position(),
            format!("Expected identifier, got nothing"),
        ))
    } else {
        Ok(res)
    }
}

pub fn read_string<R: Read>(s: &mut Scanner<R>, n: usize) -> Result<String> {
    let mut res = String::with_capacity(n);
    for _ in 0..n {
        match s.current() {
            Some(d) => {
                res.push(d);
                s.advance()?
            }
            None => {
                return Err(ParserError::Unexpected(
                    s.position(),
                    format!("Expected more input, got EOF"),
                ))
            }
        }
    }
    Ok(res)
}

pub fn consume_eol<R: Read>(s: &mut Scanner<R>) -> Result<()> {
    if s.current().is_none() {
        Ok(())
    } else {
        consume_char(s, '\n')
    }
}

pub fn consume_space1<R: Read>(s: &mut Scanner<R>) -> Result<()> {
    if let Some(c) = s.current() {
        if !c.is_ascii_whitespace() {
            return Err(ParserError::Unexpected(
                s.position(),
                format!("Expected white space, got '{}'", c),
            ));
        }
    }
    consume_space(s)
}

pub fn consume_space<R: Read>(s: &mut Scanner<R>) -> Result<()> {
    consume_while(s, |c| *c != '\n' && c.is_ascii_whitespace())
}

pub fn consume_rest_of_line<R: Read>(s: &mut Scanner<R>) -> Result<()> {
    consume_while(s, |c| *c != '\n')?;
    consume_eol(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_while() -> Result<()> {
        let mut s = Scanner::new("asdf".as_bytes());
        s.advance()?;
        assert_eq!(read_while(&mut s, |&c| c != 'f')?, "asd");
        Ok(())
    }

    #[test]
    fn test_consume_while() -> Result<()> {
        let mut s = Scanner::new("asdf".as_bytes());
        s.advance()?;
        consume_while(&mut s, |&c| c != 'f')?;
        consume_char(&mut s, 'f')?;
        assert!(s.current().is_none());
        Ok(())
    }

    #[test]
    fn test_consume_char() -> Result<()> {
        let bs = "asdf".as_bytes();
        let mut s = Scanner::new(bs);
        s.advance()?;
        for codepoint in CodePoints::from(bs) {
            let cp = codepoint.map_err(|e| ParserError::IO(s.position(), e))?;
            consume_char(&mut s, cp)?
        }
        Ok(())
    }

    #[test]
    fn test_consume_string() -> Result<()> {
        let tests = ["asdf"];
        for test in tests.iter() {
            let mut s = Scanner::new(test.as_bytes());
            s.advance()?;
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
        for (test, expected) in tests.iter() {
            let mut s = Scanner::new(test.as_bytes());
            s.advance()?;
            assert_eq!(read_quoted_string(&mut s)?, *expected);
        }
        let tests = ["\""];
        for test in tests.iter() {
            let mut s = Scanner::new(test.as_bytes());
            s.advance()?;
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
        for (test, expected) in tests.iter() {
            let mut s = Scanner::new(test.as_bytes());
            s.advance()?;
            assert_eq!(read_identifier(&mut s)?, *expected);
        }
        let tests = [" "];
        for test in tests.iter() {
            let mut s = Scanner::new(test.as_bytes());
            s.advance()?;
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
        for (test, expected, remainder) in tests.iter() {
            let mut s = Scanner::new(test.as_bytes());
            s.advance()?;
            assert_eq!(read_string(&mut s, 4)?, *expected);
            assert_eq!(read_all(&mut s)?.as_str(), *remainder)
        }
        for (test, _, _) in tests.iter() {
            let mut s = Scanner::new(test.as_bytes());
            s.advance()?;
            assert!(read_string(&mut s, test.len() + 1).is_err());
            assert_eq!(read_all(&mut s)?.as_str(), "")
        }
        Ok(())
    }

    #[test]
    fn test_consume_eol() -> Result<()> {
        let tests = ["", "\n", "\na"];
        for test in tests.iter() {
            let mut s = Scanner::new(test.as_bytes());
            s.advance()?;
            consume_eol(&mut s)?
        }
        let tests = [" ", "not an eol", "na"];
        for test in tests.iter() {
            let mut s = Scanner::new(test.as_bytes());
            s.advance()?;
            assert!(consume_eol(&mut s).is_err())
        }
        Ok(())
    }

    #[test]
    fn test_consume_space1() -> Result<()> {
        let tests = ["", "\n", "\t"];
        for test in tests.iter() {
            let mut s = Scanner::new(test.as_bytes());
            s.advance()?;
            consume_space1(&mut s)?
        }
        let tests = ["a\n", "n", "na"];
        for test in tests.iter() {
            let mut s = Scanner::new(test.as_bytes());
            s.advance()?;
            assert!(consume_space1(&mut s).is_err())
        }
        Ok(())
    }
}
