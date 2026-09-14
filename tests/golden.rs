//! Golden tests for the importers.
//!
//! Test cases live in `testdata/<root>/<importer>/<case>/`, where `<root>` is
//! either `public` (checked into this repository) or `private` (a git
//! submodule containing real bank statements, see README). Each case
//! directory contains:
//!
//! - `case.yaml`: the `fin import` command line to run, e.g.
//!   `args: [ch.postfinance, --account, Assets:Bank, statement.csv]`.
//!   File arguments are relative to the case directory.
//! - the input file(s) named in `args`
//! - `expected.journal`: the expected importer output
//!
//! Run with `UPDATE_GOLDEN=1 cargo test --test golden` to (re)generate the
//! `expected.journal` files from the current importer output.

use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

use clap::Parser;
use serde::Deserialize;

const ROOTS: &[&str] = &["testdata/public", "testdata/private"];

#[derive(Deserialize)]
struct Case {
    args: Vec<String>,
}

/// Mirrors the `fin import` subcommand so cases are parsed with the same
/// clap definitions as the binary.
#[derive(Parser)]
#[command(name = "import", no_binary_name = true)]
struct Import {
    #[command(subcommand)]
    command: fin::importer::Commands,
}

#[test]
fn golden() {
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let cases = collect_cases();
    assert!(!cases.is_empty(), "no golden test cases found");
    let mut failures = Vec::new();
    for dir in &cases {
        match run_case(dir, update) {
            Ok(()) => eprintln!("ok   {}", dir.display()),
            Err(e) => {
                eprintln!("FAIL {}", dir.display());
                failures.push(format!("{}:\n{e}", dir.display()));
            }
        }
    }
    if !failures.is_empty() {
        panic!(
            "{} of {} golden cases failed\n\n{}",
            failures.len(),
            cases.len(),
            failures.join("\n\n")
        );
    }
}

/// Returns all `<root>/<importer>/<case>` directories. Missing roots (e.g.
/// an uninitialized private submodule) are skipped.
fn collect_cases() -> Vec<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut cases = ROOTS
        .iter()
        .flat_map(|root| subdirs(&manifest.join(root)))
        .flat_map(|importer| subdirs(&importer))
        .filter(|case| case.join("case.yaml").is_file())
        .collect::<Vec<_>>();
    cases.sort();
    cases
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect()
}

fn run_case(dir: &Path, update: bool) -> Result<(), Box<dyn Error>> {
    let case: Case = serde_yaml::from_reader(fs::File::open(dir.join("case.yaml"))?)?;
    let import = Import::try_parse_from(&case.args)?;
    let mut actual = Vec::new();
    // Run in the case directory so file arguments resolve relative to it.
    // This is process-global state, which is fine as long as `golden` is the
    // only test in this binary.
    let cwd = std::env::current_dir()?;
    std::env::set_current_dir(dir)?;
    let result = import.command.run(&mut actual);
    std::env::set_current_dir(cwd)?;
    result?;
    let actual = String::from_utf8(actual)?;
    let expected_path = dir.join("expected.journal");
    if update {
        fs::write(&expected_path, &actual)?;
        return Ok(());
    }
    let expected = fs::read_to_string(&expected_path).map_err(|e| {
        format!(
            "{}: {e} (run with UPDATE_GOLDEN=1 to create it)",
            expected_path.display()
        )
    })?;
    if actual != expected {
        return Err(format!(
            "output differs from {}:\n{}",
            expected_path.display(),
            pretty_assertions::StrComparison::new(&expected, &actual)
        )
        .into());
    }
    Ok(())
}
