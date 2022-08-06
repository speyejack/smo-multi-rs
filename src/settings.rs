use std::{collections::HashSet, net::IpAddr, sync::Arc};

use tokio::sync::RwLock;

use crate::guid::Guid;

#[derive(Clone, Debug, Default)]
pub struct Settings {
    pub max_players: u16,
    pub banned_players: HashSet<Guid>,
    pub banned_ips: HashSet<IpAddr>,
}

pub type SyncSettings = Arc<RwLock<Settings>>;
