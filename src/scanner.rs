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
                Ok(())
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
    read_while(s, |c| c.is_alphanumeric())
}
