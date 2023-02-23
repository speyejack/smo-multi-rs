use crate::{
    client::{Client, PlayerData},
    guid::Guid,
    net::Packet,
};

use tokio::sync::mpsc;

use super::ClientCommand;

#[derive(Debug)]
pub enum ServerCommand {
    NewPlayer {
        cli: Client,
        data: PlayerData,
        connect_packet: Box<Packet>,
        comm: mpsc::Sender<ClientCommand>,
    },
    DisconnectPlayer {
        guid: Guid,
    },
}
