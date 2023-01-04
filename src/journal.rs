use std::{
    error::Error,
    fs, io,
    path::PathBuf,
    sync::mpsc,
    thread::{self, JoinHandle},
};

use crate::{
    model::Command,
    parser::{Directive, Parser},
    scanner::ParserError,
};

#[derive(Debug)]
pub enum JournalError {
    ParserError(ParserError),
    IOError(io::Error),
}

impl std::fmt::Display for JournalError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::ParserError(e) => e.fmt(f),
            Self::IOError(e) => e.fmt(f),
        }
    }
}

impl Error for JournalError {}

pub type Result<T> = std::result::Result<T, JournalError>;

pub fn read_from_file(p: PathBuf) -> (mpsc::Receiver<Result<Vec<Command>>>, JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    (rx, thread::spawn(move || parse_spawn(p, tx)))
}

pub fn parse_spawn(p: PathBuf, tx: mpsc::Sender<Result<Vec<Command>>>) {
    match parse_and_separate(p) {
        Ok((commands, includes)) => {
            tx.send(Ok(commands)).unwrap();
            includes
                .into_iter()
                .map(|i| {
                    let tx = tx.clone();
                    thread::spawn(move || parse_spawn(i, tx))
                })
                .collect::<Vec<_>>()
                .into_iter()
                .for_each(|t| t.join().unwrap());
        }
        Err(e) => {
            tx.send(Err(e)).unwrap();
        }
    }
}

pub fn parse_and_separate(p: PathBuf) -> Result<(Vec<Command>, Vec<PathBuf>)> {
    let text = fs::read_to_string(&p).map_err(JournalError::IOError)?;
    let mut s = Parser::new_from_file(&text, Some(p.clone()));
    let ds = s.parse().map_err(JournalError::ParserError)?;
    let mut cs = Vec::new();
    let mut is = Vec::new();
    for d in ds {
        match d {
            Directive::Command(c) => cs.push(c),
            Directive::Include(i) => is.push(p.parent().unwrap().join(i)),
        }
    }
    Ok((cs, is))
}
