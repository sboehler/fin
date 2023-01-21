use std::{
    error::Error,
    fs, io,
    path::PathBuf,
    sync::{mpsc, Arc},
    thread::{self, JoinHandle},
};

use crate::{
    context::Context,
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

pub fn read_from_file(p: PathBuf) -> (mpsc::Receiver<Result<Command>>, JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    let context = Arc::new(Context::new());
    (rx, thread::spawn(move || parse_spawn(context, p, tx)))
}

pub fn parse_spawn(context: Arc<Context>, p: PathBuf, tx: mpsc::Sender<Result<Command>>) {
    match fs::read_to_string(&p) {
        Ok(text) => {
            let s = Parser::new_from_file(context.clone(), &text, Some(p.clone()));
            let mut jh = Vec::new();
            for dir in s {
                match dir {
                    Ok(Directive::Command(c)) => tx.send(Ok(c)).unwrap(),
                    Ok(Directive::Include(i)) => {
                        let tx = tx.clone();
                        let i = p.parent().unwrap().join(i);
                        let context = context.clone();
                        jh.push(thread::spawn(move || parse_spawn(context, i, tx)));
                    }
                    Err(err) => tx.send(Err(JournalError::ParserError(err))).unwrap(),
                }
            }
            for j in jh {
                j.join().unwrap()
            }
        }
        Err(e) => tx.send(Err(JournalError::IOError(e))).unwrap(),
    };
}
