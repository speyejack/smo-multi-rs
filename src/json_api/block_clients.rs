use lazy_static::lazy_static;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use tokio::sync::RwLock;

lazy_static! {
    static ref FAILURES: RwLock<HashMap<IpAddr, u8>> = RwLock::new(HashMap::new());
}

const MAX_TRIES: u8 = 5;

pub struct BlockClients {}

impl BlockClients {
    pub async fn is_blocked(addr: &SocketAddr) -> bool {
        match FAILURES.read().await.get(&addr.ip()) {
            Some(failures) => failures >= &MAX_TRIES,
            None => false,
        }
    }

    pub async fn fail(addr: &SocketAddr) {
        let ip = addr.ip();
        let failures: u8 = *FAILURES
            .write()
            .await
            .entry(ip)
            .and_modify(|x| *x += 1)
            .or_insert(1);
        if failures >= MAX_TRIES {
            tracing::warn!("Block client {} because of too many failed requests.", ip);
        }
    }

    pub async fn redeem(addr: &SocketAddr) {
        FAILURES.write().await.remove(&addr.ip());
    }
}
