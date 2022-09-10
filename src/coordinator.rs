use crate::{
    client::SyncPlayer,
    cmds::{Command, ServerCommand},
    guid::Guid,
    net::{connection, fixedStr::FixedString, ConnectionType, Packet, PacketData},
    settings::SyncSettings,
    types::{ClientInitError, Result, SMOError},
};

use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
    sync::Arc,
    time::Duration,
};
use tokio::sync::{mpsc, RwLock};
use tracing::{info_span, Instrument};
type SyncShineBag = Arc<RwLock<HashSet<i32>>>;

pub struct Coordinator {
    pub shine_bag: SyncShineBag,
    pub settings: SyncSettings,
    pub clients: HashMap<Guid, (mpsc::Sender<Command>, SyncPlayer)>,
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
                    PacketData::Costume(_) => {
                        self.sync_all_shines().await?;
                    }
                    PacketData::Shine { shine_id, .. } => {
                        self.shine_bag.write().await.insert(*shine_id);
                        tracing::info!("Got moon {shine_id}");
                        self.sync_all_shines().await?;

                        return Ok(true);
                    }
                    PacketData::Game {
                        is_2d: _,
                        scenario_num,
                        stage,
                    } => {
                        if stage.as_ref() == "CapWorldHomeStage" && *scenario_num == 0 {
                            let client = self.get_client(&packet.id)?;
                            let mut data = client.write().await;
                            tracing::info!("Player '{}' starting speedrun", data.name);
                            data.speedrun_start = true;
                            data.shine_sync.clear();
                            drop(data);
                            self.shine_bag.write().await.clear();
                            self.persist_shines().await;
                        } else if stage.as_ref() == "WaterfallWordHomeStage" {
                            let client = self.get_client(&packet.id)?;
                            let mut data = client.write().await;
                            tracing::info!("Enabling shine sync for player '{}'", data.name);
                            let was_speed_run = data.speedrun_start;
                            data.speedrun_start = false;
                            drop(data);

                            if was_speed_run {
                                let client = client.clone();
                                let channel = self.get_channel(&packet.id)?.clone();
                                let shine_bag = self.shine_bag.clone();
                                tokio::spawn(async move {
                                    tokio::time::sleep(Duration::from_secs(15)).await;

                                    let result =
                                        client_sync_shines(channel, shine_bag, &packet.id, &client)
                                            .await;
                                    if let Err(e) = result {
                                        tracing::warn!("Initial shine sync failed: {e}")
                                    }
                                });
                            }

                            let merge_scenario = client
                                .read()
                                .await
                                .settings
                                .read()
                                .await
                                .scenario
                                .merge_enabled;
                            if merge_scenario {
                                self.merge_scenario(&packet).await?;
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

    async fn merge_scenario(&self, packet: &Packet) -> Result<()> {
        for (guid, (channel, client)) in &self.clients {
            let mut packet = packet.clone();
            let scenario_num = client.read().await.scenario;
            match &mut packet.data {
                PacketData::ChangeStage {
                    ref mut scenerio, ..
                } => {
                    *scenerio = scenario_num;
                }
                _ => {}
            }

            channel.send(Command::Packet(packet)).await?;
        }
        Ok(())
    }

    async fn persist_shines(&self) {
        // TODO
        tracing::warn!("Shine persisting not avaliable.")
    }

    fn get_client(&self, id: &Guid) -> std::result::Result<&SyncPlayer, SMOError> {
        self.clients
            .get(id)
            .map(|x| &x.1)
            .ok_or(SMOError::InvalidID(*id))
    }

    fn get_channel(&self, id: &Guid) -> std::result::Result<&mpsc::Sender<Command>, SMOError> {
        self.clients
            .get(id)
            .map(|x| &x.0)
            .ok_or(SMOError::InvalidID(*id))
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
            PacketData::Connect {
                c_type,
                client_name,
                ..
            } => (c_type, client_name),
            _ => unreachable!(),
        };

        // Verify client allowed to connect
        let can_connect = {
            let settings = self.settings.read().await;
            let max_players: usize = settings.server.max_players.into();
            let banned_players = &settings.ban_list.players;
            let banned_ips = &settings.ban_list.ips;

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

        let data = match connection_type {
            ConnectionType::FirstConnection => cli.player.clone(),
            ConnectionType::Reconnecting => match self.clients.remove(&id) {
                Some((_, prev_data)) => {
                    cli.player = prev_data.clone();
                    prev_data
                }
                None => cli.player.clone(),
            },
        };
        self.clients.insert(id, (comm.clone(), data));

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

    async fn setup_player(&mut self, comm: mpsc::Sender<Command>, packet: Packet) -> Result<()> {
        tracing::debug!(
            "Setting up player ({}) with {} other players",
            packet.id,
            self.clients.len() - 1,
        );
        let settings = self.settings.read().await;
        let max_player = settings.server.max_players;

        drop(settings);
        // Sync connection, costumes, and last game packet
        for (other_id, (_, other_cli)) in self.clients.iter() {
            let other_cli = other_cli.read().await;

            let connect_packet = Packet::new(
                *other_id,
                PacketData::Connect {
                    c_type: ConnectionType::FirstConnection,
                    max_player,
                    client_name: other_cli.name.clone().into(),
                },
            );

            let costume_packet =
                Packet::new(*other_id, PacketData::Costume(other_cli.costume.clone()));

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
        if let Some((comm, _)) = self.clients.remove(&guid) {
            let packet = Packet::new(guid, PacketData::Disconnect);
            self.broadcast(packet.clone()).await?;
            let disconnect = Command::Packet(packet);
            comm.send(disconnect).await?;
        }

        Ok(())
    }

    async fn sync_all_shines(&mut self) -> Result<()> {
        for (guid, (channel, player)) in &self.clients {
            let sender_guid = Guid::default();
            client_sync_shines(
                channel.clone(),
                self.shine_bag.clone(),
                &sender_guid,
                player,
            )
            .await?;
        }
        Ok(())
    }

    async fn broadcast(&mut self, mut p: Packet) -> Result<()> {
        p.resize();
        for (cli, _) in &mut self.clients.values() {
            cli.send(Command::Packet(p.clone())).await?;
        }
        Ok(())
    }

    async fn shutdown(mut self) {
        let active_clients = self.clients.clone();
        for guid in active_clients.keys() {
            let _ = self.disconnect_player(*guid).await;
        }
    }
}

async fn client_sync_shines(
    to_client: mpsc::Sender<Command>,
    shine_bag: SyncShineBag,
    guid: &Guid,
    player: &SyncPlayer,
) -> Result<()> {
    let client = player.read().await;
    if client.speedrun_start {
        return Ok(());
    }

    let server_shines = shine_bag.read().await;
    let mismatch = server_shines.difference(&client.shine_sync);

    for shine_id in mismatch {
        to_client
            .send(Command::Packet(Packet::new(
                *guid,
                PacketData::Shine {
                    shine_id: *shine_id,
                    is_grand: false,
                },
            )))
            .await?;
    }
    Ok(())
}
