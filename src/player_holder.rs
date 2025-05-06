use std::collections::HashSet;
use std::{ops::Not, sync::Arc};

use bimap::BiMap;
use tokio::sync::mpsc;
use tokio::sync::RwLock;

use crate::cmds::Players;
use crate::lobby::LobbyView;
use crate::{cmds::ClientCommand, guid::Guid, types::Result};

pub(crate) type ClientChannel = mpsc::Sender<ClientCommand>;

#[derive(Clone)]
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

impl PlayerSelect<Guid> {
    pub fn into_guid_vec(self, lobby: &LobbyView) -> Result<Players> {
        let players = &lobby.get_lobby().players;
        let out = match self {
            PlayerSelect::AllPlayers => Players::All,
            PlayerSelect::SelectPlayers(players) => {
                let players = Players::Individual(players);
                players.verify(lobby)?;
                players
            }
            PlayerSelect::ExcludePlayers(not_players) => {
                let set: HashSet<_> = players.iter().map(|x| *x.key()).collect();
                let not_players = not_players.into_iter().collect();
                let players = set.difference(&not_players).copied().collect();
                Players::Individual(players)
            }
        };

        Ok(out)
    }
}

impl PlayerSelect<String> {
    pub async fn into_guid_select(self, lobby: &LobbyView) -> Result<PlayerSelect<Guid>> {
        let out = match self {
            PlayerSelect::AllPlayers => PlayerSelect::AllPlayers,
            PlayerSelect::SelectPlayers(p) => {
                let names = lobby.get_lobby().names.0.read().await;
                PlayerSelect::SelectPlayers(
                    p.iter()
                        .filter_map(|s| names.get_by_right(s))
                        .map(|g| *g)
                        .collect(),
                )
            }
            PlayerSelect::ExcludePlayers(p) => {
                let names = lobby.get_lobby().names.0.read().await;
                PlayerSelect::ExcludePlayers(
                    p.iter()
                        .filter_map(|s| names.get_by_right(s))
                        .map(|g| *g)
                        .collect(),
                )
            }
        };

        Ok(out)
    }

    pub async fn into_guid_vec(self, lobby: &LobbyView) -> Result<Players> {
        let guid_select = self.into_guid_select(lobby).await?;
        guid_select.into_guid_vec(lobby)
    }
}


impl From<Vec<Guid>> for PlayerSelect<Guid> {
    fn from(guids: Vec<Guid>) -> Self {
        PlayerSelect::SelectPlayers(guids)
    }
}

#[derive(Default, Clone, Debug)]
pub struct NameMap(pub Arc<RwLock<BiMap<Guid, String>>>);
