use lazy_static::lazy_static;
use std::collections::HashMap;
use std::net::IpAddr;
use tokio::sync::RwLock;


use crate::net::connection::Connection;


lazy_static! {
    static ref FAILURES: RwLock<HashMap<IpAddr, u8>> = RwLock::new(HashMap::new());
    static ref MAX_TRIES: u8 = 5;
}


pub struct BlockClients {
}


impl BlockClients {
    pub async fn is_blocked(conn: &Connection) -> bool {
        match FAILURES.read().await.get(&conn.addr.ip()) {
            Some(failures) => failures >= &MAX_TRIES,
            None => false,
        }
    }

    pub async fn fail(conn: &Connection) {
        let ip = conn.addr.ip();
        let failures: u8 = *FAILURES.write().await.entry(ip).and_modify(|x| *x += 1).or_insert(1);
        if failures >= *MAX_TRIES {
            tracing::warn!("Block client {} because of too many failed requests.", ip);
        }
    }

    pub async fn redeem(conn: &Connection) {
        FAILURES.write().await.remove(&conn.addr.ip());
    }
}
