extern crate unicode_reader;
use std::fs::File;

mod model;
mod scanner;

fn main() {
    let path = "journal.bean";
    let file = File::open(path).expect("Could not open file");
    let p = scanner::Scanner::new(file);
    for g in p {
        print!("{}", g.unwrap());
    }
}
