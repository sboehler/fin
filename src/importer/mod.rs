use std::{error::Error, io::Write};

use clap::Subcommand;

pub mod cashbackcards;
pub mod interactivebrokers;
pub mod postfinance;
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
            Commands::TrueWealth(command) => command.run(w),
            Commands::CashbackCards(command) => command.run(w),
        }
    }
}
