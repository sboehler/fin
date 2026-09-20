use crate::model::build_journal;
use crate::report::echarts::render_html;
use crate::report::flow::FlowBuilder;
use crate::report::mapping::{AccountMapper, Mapping};
use crate::syntax::parse_files;
use chrono::{Local, NaiveDate};
use clap::{Args, Subcommand, ValueEnum};
use rust_decimal::Decimal;
use std::io::Write;
use std::{error::Error, path::PathBuf};

#[derive(Subcommand)]
pub enum Commands {
    /// Render flows between accounts as a sankey diagram.
    Sankey(Sankey),
}

impl Commands {
    pub fn run(&self, w: &mut impl Write) -> Result<(), Box<dyn Error>> {
        match self {
            Commands::Sankey(c) => c.run(w),
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Format {
    /// A self-contained HTML page.
    Html,
    /// The ECharts option object the HTML page is built from.
    Json,
}

#[derive(Args)]
pub struct Sankey {
    path: PathBuf,

    #[arg(short, long)]
    valuation: Option<String>,

    #[arg(short, long)]
    mapping: Vec<Mapping>,

    #[arg(short = 'r', long)]
    vaccounts: Vec<String>,

    #[arg(short, long)]
    from: Option<NaiveDate>,

    #[arg(short, long)]
    to: Option<NaiveDate>,

    /// Drop flows smaller than this amount.
    #[arg(long)]
    min: Option<Decimal>,

    #[arg(long, value_enum, default_value_t = Format::Html)]
    format: Format,

    /// Output file. Defaults to stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,
}

impl Sankey {
    pub fn run(&self, w: &mut impl Write) -> Result<(), Box<dyn Error>> {
        let syntax_trees = parse_files(&self.path)?;
        let mut journal = build_journal(&syntax_trees)?;
        journal.check()?;
        let valuation = self
            .valuation
            .as_ref()
            .map(|s| journal.registry().commodity_id(s))
            .transpose()?;
        journal.process(valuation)?;
        let mapper = AccountMapper::new(journal.registry(), self.mapping.clone(), &self.vaccounts)?;
        let builder = FlowBuilder {
            from: self.from.or(journal.min_transaction_date()),
            to: self.to.unwrap_or_else(|| Local::now().date_naive()),
            mapper,
            valuated: valuation.is_some(),
            min: self.min,
        };
        let report = builder.build(&journal)?;
        for warning in &report.warnings {
            eprintln!("warning: {warning}");
        }
        let option = report.to_sankey_option();
        let out = match self.format {
            Format::Json => serde_json::to_string_pretty(&option)? + "\n",
            Format::Html => {
                let period = match builder.from {
                    Some(from) => format!("{from} to {}", builder.to),
                    None => format!("through {}", builder.to),
                };
                let subtitle = match &self.valuation {
                    Some(commodity) => format!("{period} · valued in {commodity}"),
                    None => period,
                };
                render_html(&option, "Account flows", &subtitle)?
            }
        };
        match &self.output {
            Some(path) => std::fs::write(path, out)?,
            None => w.write_all(out.as_bytes())?,
        }
        Ok(())
    }
}
