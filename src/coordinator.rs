use crate::{client::ClientMap, cmds::Command, guid::Guid, net::Packet};
use anyhow::Result;
use std::collections::HashMap;
use tokio::sync::mpsc;

pub struct Coordinator {
    pub clients: ClientMap,
    pub to_clients: HashMap<Guid, mpsc::Sender<Packet>>,
    pub from_clients: mpsc::Receiver<Command>,
}

impl Coordinator {
    pub async fn handle_commands(mut self) {
        loop {
            let packet = self.from_clients.recv().await;
            if let Some(p) = packet {
                self.handle_command(p).await;
            }
        }
    }

    async fn handle_command(&mut self, packet: Command) -> Result<()> {
        todo!()
    }
}
