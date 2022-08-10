use crate::{
    client::{ClientMap, SyncClient},
    cmds::{Command, ServerCommand},
    guid::Guid,
    net::{connection, AnyPacket, AnyPacketData, ConnectionType},
    settings::SyncSettings,
    types::{ClientInitError, Result, SMOError},
};

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};
use tokio::sync::{mpsc, RwLock};
use tracing::{info_span, Instrument};
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
                match result {
                    Ok(false) => break,
                    Ok(true) => {}
                    Err(e) => {
                        tracing::warn!("Coordinator error: {e}")
                    }
                }
            }
        }

        self.shutdown().await;
    }

    async fn handle_command(&mut self, cmd: Command) -> Result<bool> {
        match cmd {
            Command::Server(sc) => match sc {
                ServerCommand::NewPlayer { .. } => self.add_client(sc).await?,
                ServerCommand::DisconnectPlayer { guid } => self.disconnect_player(guid).await?,
                ServerCommand::Shutdown => return Ok(false),
            },
            Command::Packet(packet) => {
                match &packet.data {
                    AnyPacketData::Costume(_) => {
                        self.sync_all_shines().await?;
                    }
                    AnyPacketData::Shine { shine_id, .. } => {
                        self.shine_bag.write().await.insert(*shine_id);
                        tracing::info!("Got moon {shine_id}");
                        self.sync_all_shines().await?;

                        return Ok(true);
                    }
                    AnyPacketData::Game {
                        is_2d: _,
                        scenario_num,
                        stage,
                    } => {
                        if stage == "CapWorldHomeStage" && *scenario_num == 0 {
                            // TODO: Persist shrines
                        } else if stage == "WaterfallWordHomeStage" {
                            let client = self.get_client(&packet.header.id)?;
                            let mut data = client.write().await;
                            let was_speed_run = data.speedrun;
                            data.speedrun = true;
                            drop(data);

                            if was_speed_run {
                                let client = client.clone();
                                let channel = self.get_channel(&packet.header.id)?.clone();
                                let shine_bag = self.shine_bag.clone();
                                tokio::spawn(async move {
                                    tokio::time::sleep(Duration::from_secs(15)).await;

                                    let result = client_sync_shines(
                                        channel,
                                        shine_bag,
                                        &packet.header.id,
                                        &client,
                                    )
                                    .await;
                                    if let Err(e) = result {
                                        tracing::warn!("Initial shine sync failed: {e}")
                                    }
                                });
                            }
                        }
                    }
                    _ => {}
                };
                self.broadcast(packet).await?;
            }
            Command::Cli(_) => todo!(),
        }
        Ok(true)
    }

    fn get_client(&self, id: &Guid) -> std::result::Result<&SyncClient, SMOError> {
        self.clients.get(id).ok_or(SMOError::InvalidID(*id))
    }

    fn get_channel(&self, id: &Guid) -> std::result::Result<&mpsc::Sender<Command>, SMOError> {
        self.to_clients.get(id).ok_or(SMOError::InvalidID(*id))
    }

    async fn add_client(&mut self, cmd: ServerCommand) -> Result<()> {
        let (mut cli, packet, comm) = match cmd {
            ServerCommand::NewPlayer {
                cli,
                connect_packet,
                comm,
            } => (cli, connect_packet, comm),
            _ => unreachable!(),
        };

        let (connection_type, client_name) = match &packet.data {
            AnyPacketData::Connect {
                c_type,
                client_name,
                ..
            } => (c_type, client_name),
            _ => unreachable!(),
        };

        // Verify client allowed to connect
        let can_connect = {
            let settings = self.settings.read().await;
            let max_players: usize = settings.max_players.into();
            let banned_players = &settings.banned_players;
            let banned_ips = &settings.banned_ips;

            if max_players < self.clients.len() {
                tracing::warn!(
                    "Reached max players: {} <= {}",
                    max_players,
                    self.clients.len()
                );
                Err(SMOError::ClientInit(ClientInitError::TooManyPlayers))
            } else if banned_players.contains(&cli.guid) {
                Err(SMOError::ClientInit(ClientInitError::BannedID))
            } else if banned_ips.contains(&cli.conn.addr.ip()) {
                Err(SMOError::ClientInit(ClientInitError::BannedIP))
            } else {
                Ok(())
            }
        };

        if let Err(e) = can_connect {
            cli.disconnect().await?;
            return Err(e);
        }

        let id = cli.guid;
        match connection_type {
            ConnectionType::FirstConnection => {
                self.clients.insert(id, cli.data.clone());
            }
            ConnectionType::Reconnecting => match self.clients.get(&id) {
                Some(prev_data) => {
                    cli.data = prev_data.clone();
                }
                None => {
                    self.clients.insert(id, cli.data.clone());
                }
            },
        }
        self.to_clients.insert(id, comm.clone());

        let name = cli.display_name.clone();
        tracing::info!("New client connected: {} ({})", &name, cli.guid);
        let span = info_span!("client", name);
        tokio::spawn(async move { cli.handle_events().await }.instrument(span));

        let result = self.setup_player(comm, *packet).await;
        if let Err(e) = result {
            self.disconnect_player(id).await?;
            return Err(e);
        }
        Ok(())
    }

    async fn setup_player(&mut self, comm: mpsc::Sender<Command>, packet: AnyPacket) -> Result<()> {
        tracing::debug!(
            "Setting up player ({}) with {} other players",
            packet.header.id,
            self.clients.len()
        );
        let settings = self.settings.read().await;
        let max_player = settings.max_players;

        drop(settings);
        // Sync connection, costumes, and last game packet
        for (other_id, other_cli) in self.clients.iter() {
            let other_cli = other_cli.read().await;

            let connect_packet = AnyPacket::new(
                *other_id,
                AnyPacketData::Connect {
                    c_type: ConnectionType::FirstConnection,
                    max_player,
                    client_name: other_cli.name.clone(),
                },
            );

            let costume_packet =
                AnyPacket::new(*other_id, AnyPacketData::Costume(other_cli.costume.clone()));

            let last_game_packet = other_cli.last_game_packet.clone();

            drop(other_cli);

            comm.send(Command::Packet(connect_packet)).await?;
            comm.send(Command::Packet(costume_packet)).await?;

            if let Some(p) = last_game_packet {
                comm.send(Command::Packet(p)).await?;
            }
        }

        self.broadcast(packet).await
    }

    async fn disconnect_player(&mut self, guid: Guid) -> Result<()> {
        tracing::info!("Disconnecting player {}", guid);
        self.clients.remove(&guid);
        if let Some(comm) = self.to_clients.remove(&guid) {
            let packet = AnyPacket::new(guid, AnyPacketData::Disconnect);
            self.broadcast(packet.clone()).await?;
            let disconnect = Command::Packet(packet);
            comm.send(disconnect).await?;
        }

        Ok(())
    }

    async fn sync_all_shines(&mut self) -> Result<()> {
        for (guid, client) in &self.clients {
            let channel = self.to_clients.get(guid).unwrap();
            let sender_guid = Guid::default();
            client_sync_shines(
                channel.clone(),
                self.shine_bag.clone(),
                &sender_guid,
                client,
            )
            .await?;
        }
        Ok(())
    }

    async fn broadcast(&mut self, mut p: AnyPacket) -> Result<()> {
        p.resize();
        for cli in &mut self.to_clients.values() {
            cli.send(Command::Packet(p.clone())).await?;
        }
        Ok(())
    }

    async fn shutdown(mut self) {
        let active_clients = self.to_clients.clone();
        for guid in active_clients.keys() {
            let _ = self.disconnect_player(*guid).await;
        }
    }
}

async fn client_sync_shines(
    to_client: mpsc::Sender<Command>,
    shine_bag: SyncShineBag,
    guid: &Guid,
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
            .send(Command::Packet(AnyPacket::new(
                *guid,
                AnyPacketData::Shine {
                    shine_id: *shine_id,
                    is_grand: false,
                },
            )))
            .await?;
    }
    Ok(())
}
