use crate::{client::Client, guid::Guid, net::Packet};

use tokio::sync::mpsc;

use super::Command;

#[derive(Debug)]
pub enum ServerCommand {
    NewPlayer {
        cli: Client,
        connect_packet: Box<Packet>,
        comm: mpsc::Sender<Command>,
    },
    DisconnectPlayer {
        guid: Guid,
    },
    Shutdown,
}
