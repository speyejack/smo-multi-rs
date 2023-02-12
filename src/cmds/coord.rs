use crate::{client::Client, guid::Guid, net::Packet};
use crate::net::connection::Connection;

use tokio::sync::mpsc;

use super::ClientCommand;

#[derive(Debug)]
pub enum ServerCommand {
    NewPlayer {
        cli: Client,
        connect_packet: Box<Packet>,
        comm: mpsc::Sender<ClientCommand>,
    },
    DisconnectPlayer {
        guid: Guid,
    },
    JsonApi {
        conn: Connection,
        json: String,
    },
}
