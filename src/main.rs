extern crate fin;
extern crate unicode_reader;
use fin::scanner::Scanner;
use std::fs::File;

fn main() {
    let path = "journal.bean";
    let file = File::open(path).expect("Could not open file");
    let p = Scanner::new(file);
    for g in p {
        print!("{}", g.unwrap());
    }
}
