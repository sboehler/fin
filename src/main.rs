extern crate unicode_reader;
use std::fs::File;
use std::io::Read;
use std::io::{Bytes, Error, ErrorKind, Result};
use std::iter::Peekable;
use unicode_reader::CodePoints;

fn main() {
    let path = "journal.bean";
    let file = File::open(path).expect("Could not open file");
    let p = Scanner::new(file);
    for g in p.codepoints {
        print!("{}", g.unwrap());
    }
}

pub struct Scanner<R: Read> {
    codepoints: Peekable<CodePoints<Bytes<R>>>,
    cur: Option<char>,
    pos: (u64, u64),
}

impl<R: Read> Scanner<R> {
    pub fn new(r: R) -> Scanner<R> {
        Scanner {
            codepoints: CodePoints::from(r).peekable(),
            cur: None,
            pos: (0, 0),
        }
    }
    pub fn current(&self) -> Option<char> {
        self.cur
    }
    pub fn advance(&mut self) -> Result<()> {
        match self.codepoints.next() {
            Some(r) => match r {
                Err(e) => return Err(e),
                Ok(c) => {
                    if c == '\n' {
                        self.pos = (self.pos.0 + 1, 0)
                    } else {
                        self.pos = (self.pos.0, self.pos.1 + 1);
                    }
                    self.cur = Some(c);
                }
            },
            None => {
                self.pos = (self.pos.0, self.pos.1 + 1);
                self.cur = None;
            }
        };
        Ok(())
    }
}

impl<R: Read> Iterator for Scanner<R> {
    type Item = Result<char>;
    fn next(&mut self) -> Option<Result<char>> {
        let r = self.codepoints.next();
        if let Some(Ok('\n')) = r {
            self.pos = (self.pos.0 + 1, 0)
        } else if let Some(Ok(_)) = r {
            self.pos = (self.pos.0, self.pos.1 + 1);
        }
        r
    }
}

fn read_while<R: Read, P>(s: &mut Scanner<R>, pred: P) -> Result<String>
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

fn consume_while<R: Read, P>(s: &mut Scanner<R>, pred: P) -> Result<()>
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

fn consume_char<R: Read>(s: &mut Scanner<R>, c: char) -> Result<()> {
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

fn read_quoted_string<R: Read>(s: &mut Scanner<R>) -> Result<String> {
    consume_char(s, '"')?;
    let res = read_while(s, |c| *c != '"')?;
    consume_char(s, '"')?;
    Ok(res)
}
