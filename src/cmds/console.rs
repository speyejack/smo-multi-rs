use crate::{guid::Guid};
use std::{convert::Infallible, str::FromStr};

use clap::{Subcommand, ValueEnum};

#[derive(Subcommand, Debug, Clone)]
#[clap(rename_all = "lower")]
pub enum ConsoleCommand {
    SendAll {
        stage: String,
    },
    Send {
        stage: String,
        id: String,
        scenario: i8,
        players: Vec<PlayerSelect>,
    },
    Ban {
        players: Vec<PlayerSelect>,
    },
    Crash {
        players: Vec<PlayerSelect>,
    },
    Rejoin {
        players: Vec<PlayerSelect>,
    },
    #[clap(subcommand)]
    Scenario(ScenarioCommand),
    #[clap(subcommand)]
    Tag(TagCommand),
    MaxPlayers {
        player_count: u16,
    },
    List,
    #[clap(subcommand)]
    Flip(FlipCommand),
    #[clap(subcommand)]
    Shine(ShineCommand),
    LoadSettings,
}

#[derive(Subcommand, Debug, Clone)]
#[clap(rename_all = "lower")]
pub enum ScenarioCommand {
    Merge { enabled: Option<bool> },
}

#[derive(Subcommand, Debug, Clone)]
#[clap(rename_all = "lower")]
pub enum TagCommand {
    Time {
        player: PlayerSelect,
        minutes: u16,
        seconds: u8,
    },
    Seeking {
        player: PlayerSelect,
        is_seeking: bool,
    },
    Start {
        countdown: u8,
        seekers: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
#[clap(rename_all = "lower")]
pub enum FlipCommand {
    List,
    Add { player: Guid },
    Remove { player: Guid },
    Set { is_flipped: bool },
    Pov { value: FlipValues },
}

#[derive(Subcommand, Debug, Clone)]
#[clap(rename_all = "lower")]
pub enum ShineCommand {
    List,
    Clear,
    Sync,
    Send { id: u32, player: PlayerSelect },
}

#[derive(Debug, Clone)]
pub enum PlayerSelect {
    Player(String),
    AllPlayers,
}

impl FromStr for PlayerSelect {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(if s == "*" {
            Self::AllPlayers
        } else {
            Self::Player(s.to_string())
        })
    }
}

#[derive(ValueEnum, Clone, Debug)]
#[clap(rename_all = "lower")]
pub enum FlipValues {
    Both,
    Player,
    Others,
}

impl FromStr for FlipValues {
    type Err = Infallible;

    fn from_str(_s: &str) -> Result<Self, Self::Err> {
        unimplemented!()
    }
}
