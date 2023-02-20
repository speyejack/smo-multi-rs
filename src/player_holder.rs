use std::{collections::HashMap, ops::Not, sync::Arc};

use bimap::BiMap;
use tokio::sync::mpsc;
use tokio::sync::RwLock;

use crate::{
    client::SyncPlayer,
    cmds::ClientCommand,
    coordinator::SyncClientNames,
    guid::Guid,
    types::{Result, SMOError},
};

pub(crate) type ClientChannel = mpsc::Sender<ClientCommand>;

#[derive(Clone)]
pub(crate) struct PlayerInfo {
    pub channel: ClientChannel,
    pub data: SyncPlayer,
}

pub enum PlayerSelect<T> {
    AllPlayers,
    SelectPlayers(Vec<T>),
    ExcludePlayers(Vec<T>),
}

impl<T> Not for PlayerSelect<T> {
    type Output = Self;

    fn not(self) -> Self::Output {
        match self {
            PlayerSelect::AllPlayers => PlayerSelect::ExcludePlayers(Vec::new()),
            PlayerSelect::SelectPlayers(p) => {
                if p.is_empty() {
                    PlayerSelect::AllPlayers
                } else {
                    PlayerSelect::ExcludePlayers(p)
                }
            }
            PlayerSelect::ExcludePlayers(p) => PlayerSelect::SelectPlayers(p),
        }
    }
}

#[derive(Default)]
pub(crate) struct NameMap(pub Arc<RwLock<BiMap<Guid, String>>>);

#[derive(Default)]
pub(crate) struct PlayerHolder {
    pub clients: HashMap<Guid, PlayerInfo>,
    pub names: NameMap,
}

impl PlayerHolder {
    pub fn get_client_info(&self, id: &Guid) -> std::result::Result<&PlayerInfo, SMOError> {
        self.clients.get(id).ok_or(SMOError::InvalidID(*id))
    }

    pub fn get_client(&self, id: &Guid) -> std::result::Result<&SyncPlayer, SMOError> {
        self.get_client_info(id).map(|x| &x.data)
    }

    pub fn get_channel(&self, id: &Guid) -> std::result::Result<&ClientChannel, SMOError> {
        self.get_client_info(id).map(|x| &x.channel)
    }

    pub async fn players_to_guids(&self, players: &PlayerSelect<String>) -> Result<Vec<Guid>> {
        let client_names = self.names.0.read().await;

        let select: Result<Vec<Guid>> = match players {
            PlayerSelect::AllPlayers => Ok(client_names.left_values().copied().collect()),
            PlayerSelect::ExcludePlayers(players) => Ok(client_names
                .iter()
                .filter(|(_, s)| !players.contains(s))
                .map(|(u, _)| *u)
                .collect()),
            PlayerSelect::SelectPlayers(players) => players
                .iter()
                .map(|name| {
                    client_names
                        .get_by_right(name)
                        .copied()
                        .ok_or_else(|| SMOError::InvalidName(name.to_string()))
                })
                .collect::<Result<Vec<_>>>(),
        };
        select
    }

    pub async fn get_clients(
        &self,
        players: &PlayerSelect<String>,
    ) -> Result<PlayerSelect<&PlayerInfo>> {
        let client_names = self.names.0.read().await;

        let select = match players {
            PlayerSelect::AllPlayers => PlayerSelect::AllPlayers,
            PlayerSelect::SelectPlayers(players) => PlayerSelect::SelectPlayers(
                players
                    .iter()
                    .map(|name| {
                        let guid = client_names
                            .get_by_right(name)
                            .ok_or_else(|| SMOError::InvalidName(name.to_string()))?;
                        self.clients.get(guid).ok_or(SMOError::InvalidID(*guid))
                    })
                    .collect::<Result<_>>()?,
            ),
            PlayerSelect::ExcludePlayers(players) => PlayerSelect::SelectPlayers({
                client_names
                    .iter()
                    .filter(|(_, s)| !players.contains(s))
                    .map(|(guid, _)| self.clients.get(guid).ok_or(SMOError::InvalidID(*guid)))
                    .collect::<Result<_>>()?
            }),
        };
        Ok(select)
    }

    pub async fn flatten_players<'a>(
        &'a self,
        players: &PlayerSelect<&'a PlayerInfo>,
    ) -> Vec<&'a PlayerInfo> {
        match players {
            PlayerSelect::AllPlayers => self.clients.values().collect(),
            PlayerSelect::SelectPlayers(v) => v.to_vec(),
            PlayerSelect::ExcludePlayers(_v) => {
                unimplemented!("Exclude not implemented for player flattening");
            }
        }
    }
}
