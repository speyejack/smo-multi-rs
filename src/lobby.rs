use std::{collections::hash_map::RandomState, sync::Arc};

use dashmap::{
    mapref::one::{Ref, RefMut},
    DashMap,
};
use tokio::sync::{broadcast, mpsc};

use crate::{
    client::PlayerData,
    cmds::{Command, ServerWideCommand},
    coordinator::SyncShineBag,
    guid::Guid,
    player_holder::NameMap,
    settings::SyncSettings,
    types::{Result, SMOError},
};

pub type PlayerMap = Arc<DashMap<Guid, PlayerData>>;

#[derive(Debug)]
pub struct Lobby {
    pub settings: SyncSettings,
    pub players: PlayerMap,
    pub shines: SyncShineBag,
    pub names: NameMap,

    pub to_coord: mpsc::Sender<Command>,
    pub server_recv: broadcast::Receiver<ServerWideCommand>,
    pub lobby_broadcast: broadcast::Sender<ServerWideCommand>,
}

impl Lobby {
    pub fn new(
        settings: SyncSettings,
        to_coord: mpsc::Sender<Command>,
        lobby_broadcast: broadcast::Sender<ServerWideCommand>,
    ) -> Self {
        Self {
            settings,
            players: Default::default(),
            shines: Default::default(),
            names: Default::default(),
            to_coord,
            server_recv: lobby_broadcast.subscribe(),
            lobby_broadcast,
        }
    }

    pub fn get_client<'a>(&'a self, id: &Guid) -> Result<Ref<'a, Guid, PlayerData, RandomState>> {
        self.players.get(id).ok_or(SMOError::InvalidID(*id))
    }

    pub fn get_mut_client<'a>(
        &'a mut self,
        id: &Guid,
    ) -> Result<RefMut<'a, Guid, PlayerData, RandomState>> {
        self.players.get_mut(id).ok_or(SMOError::InvalidID(*id))
    }
}

impl Clone for Lobby {
    fn clone(&self) -> Self {
        Self {
            settings: self.settings.clone(),
            players: self.players.clone(),
            shines: self.shines.clone(),
            names: self.names.clone(),
            to_coord: self.to_coord.clone(),
            server_recv: self.lobby_broadcast.subscribe(),
            lobby_broadcast: self.lobby_broadcast.clone(),
        }
    }
}

#[derive(Clone)]
pub struct LobbyView {
    lobby: Lobby,
}

impl LobbyView {
    pub fn new(lobby: &Lobby) -> Self {
        Self {
            lobby: lobby.clone(),
        }
    }

    pub fn get_lobby(&self) -> &Lobby {
        &self.lobby
    }

    pub fn get_mut_settings(&mut self) -> &mut SyncSettings {
        &mut self.lobby.settings
    }

    pub fn get_mut_client<'a>(&'a mut self, id: &Guid) -> Result<RefMut<'a, Guid, PlayerData, RandomState>> {
        self.lobby.get_mut_client(id)
    }

    pub fn get_server_recv(&mut self) -> &mut broadcast::Receiver<ServerWideCommand> {
        &mut self.lobby.server_recv
    }

    pub fn get_server_send(&mut self) -> &mut broadcast::Sender<ServerWideCommand> {
        &mut self.lobby.lobby_broadcast
    }

    pub fn get_coord_send(&mut self) -> &mut mpsc::Sender<Command> {
        &mut self.lobby.to_coord
    }
}
