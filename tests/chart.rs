//! Golden tests for the chart reports.
//!
//! Test cases live in `testdata/public/chart/<case>/`, each containing:
//!
//! - `chart.yaml`: the `fin chart` command line to run, e.g.
//!   `args: [sankey, ../journal.fin, --format, json]`. File arguments are
//!   relative to the case directory.
//! - `expected.json`: the expected ECharts option object.
//!
//! The manifest is deliberately named `chart.yaml` rather than `case.yaml` so
//! that `tests/golden.rs`, which scans for importer cases, skips these.
//!
//! Only the JSON is compared: it is the entirety of the Rust-side output, and
//! the HTML renderer only wraps it in a fixed, vendored template.
//!
//! Run with `UPDATE_GOLDEN=1 cargo test --test chart` to (re)generate the
//! `expected.json` files.

use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

use clap::Parser;
use serde::Deserialize;

const ROOT: &str = "testdata/public/chart";

#[derive(Deserialize)]
struct Case {
    args: Vec<String>,
}

/// Mirrors the `fin chart` subcommand so cases are parsed with the same clap
/// definitions as the binary.
#[derive(Parser)]
#[command(name = "chart", no_binary_name = true)]
struct Chart {
    #[command(subcommand)]
    command: fin::commands::chart::Commands,
}

#[test]
fn chart() {
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let cases = collect_cases();
    assert!(!cases.is_empty(), "no chart test cases found");
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
            "{} of {} chart cases failed\n\n{}",
            failures.len(),
            cases.len(),
            failures.join("\n\n")
        );
    }
}

fn collect_cases() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(ROOT);
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut cases = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.join("chart.yaml").is_file())
        .collect::<Vec<_>>();
    cases.sort();
    cases
}

fn run_case(dir: &Path, update: bool) -> Result<(), Box<dyn Error>> {
    let case: Case = serde_yaml::from_reader(fs::File::open(dir.join("chart.yaml"))?)?;
    let chart = Chart::try_parse_from(&case.args)?;
    let mut actual = Vec::new();
    // Run in the case directory so file arguments resolve relative to it.
    // This is process-global state, which is fine as long as `chart` is the
    // only test in this binary.
    let cwd = std::env::current_dir()?;
    std::env::set_current_dir(dir)?;
    let result = chart.command.run(&mut actual);
    std::env::set_current_dir(cwd)?;
    result?;
    let actual = String::from_utf8(actual)?;
    let expected_path = dir.join("expected.json");
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
