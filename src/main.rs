extern crate clap;
extern crate fin;
extern crate unicode_reader;
use clap::{App, Arg};
use fin::scanner::Scanner;
use std::fs::File;

fn main() {
    let matches = App::new("fin")
        .version("0.1")
        .about("A personal finance tool")
        .author("Silvio Böhler")
        .subcommand(
            App::new("print").about("pretty prints the journal").arg(
                Arg::with_name("JOURNAL")
                    .help("the journal file to use")
                    .required(true)
                    .index(1),
            ),
        )
        .subcommand(App::new("balance").about("print a balance"))
        .get_matches();

    if let Some(print_cmd) = matches.subcommand_matches("print") {
        let path = print_cmd.value_of("JOURNAL").unwrap();
        let file = File::open(path).expect("Could not open file");
        let p = Scanner::new(file);
        for g in p {
            print!("{}", g.unwrap());
        }
    }
}
