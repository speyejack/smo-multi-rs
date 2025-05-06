pub mod client;
pub mod console;
pub mod coord;
pub mod reply;

pub use client::ClientCommand;
pub use console::ConsoleCommand;
pub use coord::ServerCommand;

use crate::{
    guid::Guid,
    lobby::{Lobby, LobbyView},
    net::Packet,
    types::{Result, SMOError},
};

use std::{collections::BTreeSet, net::IpAddr};

use self::reply::ReplyChannel;

#[derive(Debug)]
pub enum Command {
    Packet(Packet),
    External(ExternalCommand, ReplyChannel<Result<String>>),
    Server(ServerCommand),
}

#[derive(Debug, Clone)]
pub enum ServerWideCommand {
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum ExternalCommand {
    Player {
        players: Players,
        command: PlayerCommand,
    },
    Shine {
        command: ShineCommand,
    },
}

#[derive(Debug, Clone)]
pub enum PlayerCommand {
    Send {
        stage: String,
        id: String,
        scenario: i8,
    },
    Disconnect {},
    Crash {},
    Tag {
        time: Option<(u16, u8)>,
        is_seeking: Option<bool>,
    },
    SendShine {
        id: i32,
    },
}

#[derive(Debug, Clone)]
pub enum ShineCommand {
    Sync,
    Clear,
}

#[derive(Debug, Clone)]
pub enum Players {
    All,
    Individual(Vec<Guid>),
}

impl Players {
    pub fn flatten(self, lobby: &Lobby) -> Result<Vec<Guid>> {
        match self {
            Self::All => Ok(lobby.players.iter().map(|x| *x.key()).collect()),
            Self::Individual(p) => Ok(p),
        }
    }

    pub fn get_guids(&self, lobby: &Lobby) -> BTreeSet<Guid> {
        match self {
            Self::All           => lobby.players.iter().map(|x| *x.key()).collect(),
            Self::Individual(p) => p.iter().cloned().collect(),
        }
    }

    pub fn get_ipv4s(&self, lobby: &Lobby) -> BTreeSet<IpAddr> {
        match self {
            Self::All           => lobby.players.iter().filter_map(|x| x.value().ipv4).collect(),
            Self::Individual(p) => lobby.players.iter().filter(|x| p.contains(x.key())).filter_map(|x| x.value().ipv4).collect(),
        }
    }

    pub fn get_names(&self, lobby: &Lobby) -> BTreeSet<String> {
        match self {
            Self::All           => lobby.players.iter().map(|x| x.value().name.to_string()).collect(),
            Self::Individual(p) => lobby.players.iter().filter(|x| p.contains(x.key())).map(|x| x.value().name.to_string()).collect(),
        }
    }

    pub fn verify(&self, lobby: &LobbyView) -> Result<()> {
        match self {
            Self::All => Ok(()),
            Self::Individual(p) => {
                for guid in p {
                    lobby
                        .get_lobby()
                        .players
                        .contains_key(guid)
                        .then_some(())
                        .ok_or(SMOError::InvalidID(*guid))?
                }
                Ok(())
            }
        }
    }
}
