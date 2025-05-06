use crate::{guid::Guid, net::GameMode, player_holder::PlayerSelect, settings::FlipPovSettings};
use std::{convert::Infallible, net::IpAddr, str::FromStr};

use clap::Subcommand;

#[derive(Subcommand, Debug, Clone)]
#[clap(rename_all = "lower")]
pub enum ConsoleCommand {
    SendAll {
        #[arg(short, long)]
        force: bool,
        stage: String,
    },
    Send {
        #[arg(short, long)]
        force: bool,
        stage: String,
        id: String,
        scenario: i8,
        players: Vec<SinglePlayerSelect>,
    },
    #[clap(subcommand)]
    Ban(BanCommand),
    #[clap(subcommand)]
    Unban(UnbanCommand),
    Crash {
        players: Vec<SinglePlayerSelect>,
    },
    Rejoin {
        players: Vec<SinglePlayerSelect>,
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
    Shine(ShineArg),
    #[clap(subcommand)]
    Udp(UdpCommand),
    LoadSettings,
    Restart,
}

#[derive(Subcommand, Debug, Clone)]
#[clap(rename_all = "lower")]
pub enum BanCommand {
    List,
    Enable,
    Disable,
    Player {
        players: Vec<SinglePlayerSelect>,
    },
    Profile {
        profile_id: Guid,
    },
    IP {
        ipv4: IpAddr,
    },
    Stage {
        stage: String,
    },
    GameMode {
        game_mode: GameMode,
    },
}

#[derive(Subcommand, Debug, Clone)]
#[clap(rename_all = "lower")]
pub enum UnbanCommand {
    Profile {
        profile_id: Guid,
    },
    IP {
        ipv4: IpAddr,
    },
    Stage {
        stage: String,
    },
    GameMode {
        game_mode: GameMode,
    },
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
        player: SinglePlayerSelect,
        minutes: u16,
        seconds: u8,
    },
    Seeking {
        player: SinglePlayerSelect,
        #[arg(action = clap::ArgAction::Set)]
        is_seeking: bool,
    },
    Start {
        countdown: u8,
        seekers: Vec<SinglePlayerSelect>,
    },
}

#[derive(Subcommand, Debug, Clone)]
#[clap(rename_all = "lower")]
pub enum FlipCommand {
    List,
    Add {
        player: Guid,
    },
    Remove {
        player: Guid,
    },
    Set {
        #[arg(action = clap::ArgAction::Set)]
        is_flipped: bool,
    },
    Pov {
        value: FlipPovSettings,
    },
}

#[derive(Subcommand, Debug, Clone)]
#[clap(rename_all = "lower")]
pub enum ShineArg {
    List,
    Clear,
    Sync,
    Send {
        id: i32,
        player: SinglePlayerSelect,
    },
    Set {
        #[arg(action = clap::ArgAction::Set)]
        should_sync: bool,
    },
    Include {
        id: i32,
    },
    Exclude {
        id: i32,
    },
}

#[derive(Subcommand, Debug, Clone)]
#[clap(rename_all = "lower")]
pub enum UdpCommand {
    Init {
        player: SinglePlayerSelect,
    },
    Auto {
        #[arg(action = clap::ArgAction::Set)]
        should_auto: bool,
    },
}

#[derive(Debug, Clone)]
pub enum SinglePlayerSelect {
    Player(String),
    Negate,
    AllPlayers,
}

impl ToString for SinglePlayerSelect {
    fn to_string(&self) -> String {
        match self {
            SinglePlayerSelect::Player(p) => p,
            SinglePlayerSelect::Negate => "!",
            SinglePlayerSelect::AllPlayers => "*",
        }
        .to_string()
    }
}

impl FromStr for SinglePlayerSelect {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(if s == "*" {
            Self::AllPlayers
        } else if s == "!" {
            Self::Negate
        } else {
            Self::Player(s.to_string())
        })
    }
}

impl From<&[SinglePlayerSelect]> for PlayerSelect<String> {
    fn from(players: &[SinglePlayerSelect]) -> Self {
        let modifier = players.iter().next();
        match modifier {
            Some(SinglePlayerSelect::AllPlayers) => PlayerSelect::AllPlayers,
            Some(SinglePlayerSelect::Negate) => {
                let players: Vec<_> = players
                    .into_iter()
                    .skip(1)
                    .map(SinglePlayerSelect::to_string)
                    .collect();

                if players.is_empty() {
                    PlayerSelect::AllPlayers
                } else {
                    PlayerSelect::ExcludePlayers(players)
                }
            }
            _ => {
                let players = players
                    .into_iter()
                    .map(SinglePlayerSelect::to_string)
                    .collect();

                PlayerSelect::SelectPlayers(players)
            }
        }
    }
}
