use crate::{
    client::{ClientMap, SyncClient},
    cmds::{Command, ServerCommand},
    guid::Guid,
    net::{Packet, PacketData},
    settings::SyncSettings,
    types::SMOError,
};
use anyhow::Result;
use dashmap::Map;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};
use tokio::sync::{mpsc, RwLock};
type SyncShineBag = Arc<RwLock<HashSet<i32>>>;

pub struct Coordinator {
    pub shine_bag: SyncShineBag,
    pub settings: SyncSettings,
    pub clients: ClientMap,
    pub to_clients: HashMap<Guid, mpsc::Sender<Command>>,
    pub from_clients: mpsc::Receiver<Command>,
}

impl Coordinator {
    pub async fn handle_commands(mut self) {
        loop {
            let cmd = self.from_clients.recv().await;
            if let Some(c) = cmd {
                let result = self.handle_command(c).await;

                if let Err(e) = result {
                    log::warn!("Coordinator error: {e}")
                }
            }
        }
    }

    async fn handle_command(&mut self, cmd: Command) -> Result<()> {
        match cmd {
            Command::Server(ServerCommand::NewPlayer { .. }) => {
                self.add_client(cmd).await?;
            }
            Command::Packet(packet) => {
                match &packet.data {
                    PacketData::Costume(_) => {
                        // TODO Sync client
                    }
                    PacketData::Shine { shine_id } => {
                        self.shine_bag.write().await.insert(*shine_id);
                        let client = self
                            .clients
                            .get(&packet.id)
                            .ok_or(SMOError::InvalidID(packet.id))?;
                        let client = client.clone();
                        client_sync_shines(
                            self.get_channel(&packet.id)?.clone(),
                            self.shine_bag.clone(),
                            packet.id,
                            &client,
                        )
                        .await?;
                        // self.client_sync_shines(&client).await;
                        return Ok(());
                    }
                    PacketData::Game {
                        is_2d,
                        scenario_num,
                        stage,
                    } => {
                        if stage == "CapWorldHomeStage" && *scenario_num == 0 {
                            // TODO: Persist shrines
                        } else if stage == "WaterfallWordHomeStage" {
                            let client = self.get_client(&packet.id)?;
                            let mut data = client.write().await;
                            let was_speed_run = data.speedrun;
                            data.speedrun = true;
                            drop(data);

                            if was_speed_run {
                                let client = client.clone();
                                let channel = self.get_channel(&packet.id)?.clone();
                                let shine_bag = self.shine_bag.clone();
                                tokio::spawn(async move {
                                    tokio::time::sleep(Duration::from_secs(15)).await;

                                    let result =
                                        client_sync_shines(channel, shine_bag, packet.id, &client)
                                            .await;
                                    if let Err(e) = result {
                                        log::warn!("Initial shine sync failed: {e}")
                                    }
                                });
                            }
                        }
                    }
                    _ => {}
                };
                self.broadcast(packet).await?;
            }
            _ => todo!(),
        }
        todo!()
    }

    fn get_client(&self, id: &Guid) -> std::result::Result<&SyncClient, SMOError> {
        self.clients.get(id).ok_or(SMOError::InvalidID(*id))
    }

    fn get_channel(&self, id: &Guid) -> std::result::Result<&mpsc::Sender<Command>, SMOError> {
        self.to_clients.get(id).ok_or(SMOError::InvalidID(*id))
    }

    async fn add_client(&mut self, cmd: Command) -> Result<()> {
        todo!()
    }

    async fn sync_all_shines(&mut self) {
        unimplemented!()
    }

    async fn broadcast(&mut self, p: Packet) -> Result<()> {
        for cli in &mut self.to_clients.values() {
            cli.send(Command::Packet(p.clone())).await?;
        }
        Ok(())
    }
}

async fn client_sync_shines(
    to_client: mpsc::Sender<Command>,
    shine_bag: SyncShineBag,
    guid: Guid,
    client: &SyncClient,
) -> Result<()> {
    let client = client.read().await;
    if !client.speedrun {
        return Ok(());
    }

    let server_shines = shine_bag.read().await;
    let mismatch = server_shines.difference(&client.shine_sync);

    for shine_id in mismatch {
        to_client
            .send(Command::Packet(Packet::new(
                guid,
                PacketData::Shine {
                    shine_id: *shine_id,
                },
            )))
            .await?;
    }
    Ok(())
}
