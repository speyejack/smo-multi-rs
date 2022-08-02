use std::sync::Arc;

use tokio::sync::RwLock;

#[derive(Clone, Debug, Default)]
pub struct Settings {
    pub max_players: u16,
}
pub type SyncSettings = Arc<RwLock<Settings>>;
