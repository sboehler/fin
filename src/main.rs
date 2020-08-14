extern crate unicode_reader;
use unicode_reader::CodePoints;

fn main() {
    let path = "journal.bean";
    let file = std::fs::File::open(path).expect("Could not open file");
    let p = Parser {
        codepoints: CodePoints::from(file),
    };
    for g in p.codepoints {
        print!("{}", g.unwrap());
    }
}

struct Parser {
    codepoints: CodePoints<std::io::Bytes<std::fs::File>>,
}
