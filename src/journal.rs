use std::{
    error::Error,
    fs, io,
    path::PathBuf,
    sync::mpsc,
    thread::{self},
};

use crate::{
    model::Command,
    parser::{parse, Directive},
    scanner::{ParserError, Scanner},
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

pub fn read_from_file(p: PathBuf) -> mpsc::Receiver<Result<Vec<Command>>> {
    let (cmd_tx, cmd_rx) = mpsc::channel::<Result<Vec<Command>>>();
    thread::spawn(move || parse_spawn(p, cmd_tx));
    cmd_rx
}

pub fn parse_spawn(p: PathBuf, snd: mpsc::Sender<Result<Vec<Command>>>) {
    match parse_and_separate(p) {
        Ok((commands, includes)) => {
            snd.send(Ok(commands)).unwrap();
            includes
                .into_iter()
                .map(|i| {
                    let snd_t = snd.clone();
                    thread::spawn(move || parse_spawn(i, snd_t))
                })
                .collect::<Vec<_>>()
                .into_iter()
                .for_each(|t| t.join().unwrap());
        }
        Err(e) => {
            snd.send(Err(e)).unwrap();
        }
    }
}

pub fn parse_and_separate(p: PathBuf) -> Result<(Vec<Command>, Vec<PathBuf>)> {
    let text = fs::read_to_string(&p).map_err(JournalError::IOError)?;
    let mut s = Scanner::new_from_file(&text, Some(p.clone()));
    let ds = parse(&mut s).map_err(JournalError::ParserError)?;
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
