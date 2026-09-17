use std::{error::Error, io::Write};

use clap::Subcommand;

pub mod cashbackcards;
pub mod interactivebrokers;
pub mod postfinance;
pub mod revolut;
pub mod schwab;
pub mod truewealth;
pub mod viac;

#[derive(Subcommand)]
pub enum Commands {
    #[command(name = "ch.postfinance", about = "Import Postfinance CSV file.")]
    Postfinance(postfinance::Command),

    #[command(
        name = "com.interactivebrokers",
        about = "Import Interactive Brokers activity statement."
    )]
    InteractiveBrokers(interactivebrokers::Command),

    #[command(
        name = "ch.viac",
        about = "Import VIAC portfolio values from a JSON summary."
    )]
    Viac(viac::Command),

    #[command(
        name = "com.schwab",
        about = "Import Charles Schwab brokerage transaction export."
    )]
    Schwab(schwab::Command),

    #[command(
        name = "com.schwab.awards",
        about = "Import Charles Schwab equity awards export."
    )]
    SchwabAwards(schwab::AwardsCommand),

    #[command(
        name = "com.revolut",
        about = "Import Revolut CSV account statements, one per currency."
    )]
    Revolut(revolut::Command),

    #[command(
        name = "ch.truewealth",
        about = "Import True Wealth portfolio values from a JSON evolution export."
    )]
    TrueWealth(truewealth::Command),

    #[command(
        name = "ch.cashback-cards",
        about = "Import Swisscard Cashback Cards statement."
    )]
    CashbackCards(cashbackcards::Command),
}

impl Commands {
    pub fn run(&self, w: &mut impl Write) -> Result<(), Box<dyn Error>> {
        match self {
            Commands::Postfinance(command) => command.run(w),
            Commands::InteractiveBrokers(command) => command.run(w),
            Commands::Viac(command) => command.run(w),
            Commands::Schwab(command) => command.run(w),
            Commands::SchwabAwards(command) => command.run(w),
            Commands::Revolut(command) => command.run(w),
            Commands::TrueWealth(command) => command.run(w),
            Commands::CashbackCards(command) => command.run(w),
        }
    }
}
