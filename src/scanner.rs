use std::io::Read;
use std::io::{Bytes, Error, ErrorKind, Result};
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
        let next = self.codepoints.next().transpose()?;
        self.pos.update(self.cur, next);
        self.cur = next;
        Ok(())
    }

    pub fn position(&self) -> &Position {
        return &self.pos;
    }
}

impl<R: Read> Iterator for Scanner<R> {
    type Item = Result<char>;
    fn next(&mut self) -> Option<Result<char>> {
        match self.advance() {
            Err(e) => Some(Err(e)),
            Ok(_) => self.current().map(|c| Ok(c)),
        }
    }
}

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
                Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Expected '{}', got '{}'", c, d),
                ))
            }
        }
        None => Err(Error::new(
            ErrorKind::UnexpectedEof,
            format!("Expected '{}', got EOF", c),
        )),
    }
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
        Err(Error::new(
            ErrorKind::InvalidData,
            format!("Expected identifier, got nothing"),
        ))
    } else {
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Result;

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
        for b in CodePoints::from(bs) {
            consume_char(&mut s, b?)?
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
}
