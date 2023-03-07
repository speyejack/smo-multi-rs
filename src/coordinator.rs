use crate::{
    cmds::{
        ClientCommand, Command, ExternalCommand, PlayerCommand, Players, ServerCommand,
        ShineCommand,
    },
    guid::Guid,
    lobby::Lobby,
    net::{ConnectionType, Packet, PacketData, TagUpdate},
    player_holder::ClientChannel,
    types::Result,
};

use std::{collections::HashSet, sync::Arc, time::Duration};
use tokio::{
    fs::File,
    io::AsyncWriteExt,
    sync::{broadcast, mpsc, RwLock},
};
use tracing::{info_span, Instrument};

pub type SyncShineBag = Arc<RwLock<ShineBag>>;
pub type ShineBag = HashSet<i32>;

pub struct Coordinator {
    lobby: Lobby,
    pub from_clients: mpsc::Receiver<Command>,
    pub cli_broadcast: broadcast::Sender<ClientCommand>,
}

impl Coordinator {
    pub fn new(
        lobby: Lobby,
        from_clients: mpsc::Receiver<Command>,
        cli_broadcast: broadcast::Sender<ClientCommand>,
    ) -> Self {
        Coordinator {
            lobby,
            from_clients,
            cli_broadcast,
        }
    }
    pub async fn handle_commands(mut self) -> Result<()> {
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
        Ok(())
    }

    async fn handle_command(&mut self, cmd: Command) -> Result<bool> {
        match cmd {
            Command::Server(sc) => match sc {
                ServerCommand::NewPlayer { .. } => self.add_client(sc).await?,
                ServerCommand::DisconnectPlayer { guid } => self.disconnect_player(guid).await?,
            },
            Command::Packet(packet) => {
                match &packet.data {
                    PacketData::Costume(_) => {
                        self.sync_all_shines().await?;
                    }
                    PacketData::Shine { shine_id, .. } => {
                        self.lobby.shines.write().await.insert(*shine_id);
                        tracing::info!("Got moon {shine_id}");
                        self.sync_all_shines().await?;

                        return Ok(true);
                    }
                    PacketData::Game {
                        is_2d: _,
                        scenario_num,
                        stage,
                    } => {
                        tracing::debug!("Got game packet {}->{}", stage, scenario_num);

                        if stage == "CapWorldHomeStage" && *scenario_num == 0 {
                            let mut data = self.lobby.get_mut_client(&packet.id)?;
                            tracing::debug!("Player '{}' started new save", data.name);
                            data.value_mut().speedrun_start = true;
                            data.value_mut().shine_sync.clear();
                            drop(data);
                            let mut settings = self.lobby.shines.write().await;
                            settings.clear();
                            drop(settings);
                            self.persist_shines().await;
                        } else if stage == "WaterfallWordHomeStage" {
                            let mut data = self.lobby.get_mut_client(&packet.id)?;
                            tracing::debug!("Enabling shine sync for player '{}'", data.name);
                            let was_speed_run = data.speedrun_start;
                            data.speedrun_start = false;
                            drop(data);

                            let settings = self.lobby.settings.read().await;
                            let should_sync_shines = settings.shines.enabled;
                            drop(settings);

                            if should_sync_shines && was_speed_run {
                                let shine_bag = self.lobby.shines.clone();
                                let client_shines = self
                                    .lobby
                                    .get_client(&packet.id)?
                                    .value()
                                    .shine_sync
                                    .clone();

                                let data = self.lobby.get_client(&packet.id)?;
                                let channel = data.channel.clone();
                                drop(data);

                                tokio::spawn(async move {
                                    tokio::time::sleep(Duration::from_secs(15)).await;

                                    let result = client_sync_shines(
                                        channel,
                                        shine_bag,
                                        &packet.id,
                                        &client_shines,
                                    )
                                    .await;
                                    if let Err(e) = result {
                                        tracing::warn!("Initial shine sync failed: {e}")
                                    }
                                });
                            }
                        }
                        tracing::debug!("Changing scenarios: {} {}", scenario_num, stage);

                        let merge_scenario =
                            self.lobby.settings.read().await.scenario.merge_enabled;
                        if merge_scenario {
                            self.merge_scenario(&packet).await?;
                        }
                    }
                    _ => {}
                };
                self.broadcast(&ClientCommand::Packet(packet))?;
            }
            Command::External(cmd, reply) => {
                let result = self.handle_external_cmd(cmd).await;
                reply.send(result).expect("Reply channel failed");
            }
        }
        Ok(true)
    }

    async fn handle_external_cmd(&mut self, cmd: ExternalCommand) -> Result<String> {
        tracing::trace!("Handling external cmd");
        let out_str: String = match cmd {
            ExternalCommand::Player { players, command } => match command {
                PlayerCommand::Send {
                    stage,
                    id,
                    scenario,
                } => {
                    let data = PacketData::ChangeStage {
                        stage: stage.clone(),
                        id,
                        scenario,
                        sub_scenario: 0,
                    };
                    let packet = Packet::new(Guid::default(), data);
                    let cmd = ClientCommand::SelfAddressed(packet);
                    self.send_players(&players, &cmd).await?;
                    "Sent players".to_string()
                }
                PlayerCommand::Disconnect {} => {
                    let guids = players.flatten(&self.lobby)?;
                    for guid in guids {
                        self.disconnect_player(guid).await?;
                    }
                    "Disconnected players".to_string()
                }
                PlayerCommand::Crash {} => {
                    let data = PacketData::ChangeStage {
                        id: "$among$us/SubArea".to_string(),
                        stage: "$agogusStage".to_string(),
                        scenario: 21,
                        sub_scenario: 69, // invalid id
                    };
                    let packet = Packet::new(Guid::default(), data);
                    let cmd = ClientCommand::SelfAddressed(packet);
                    self.send_players(&players, &cmd).await?;
                    "Crashed players".to_string()
                }
                PlayerCommand::Tag { time, is_seeking } => {
                    if let Some((minutes, seconds)) = time {
                        // TODO test if is_it is the correct default
                        let tag_packet = PacketData::Tag {
                            update_type: TagUpdate::Time,
                            is_it: false,
                            minutes,
                            seconds,
                        };
                        let packet = Packet::new(Guid::default(), tag_packet);

                        self.send_players(&players, &ClientCommand::SelfAddressed(packet))
                            .await?;
                    }

                    if let Some(is_seeking) = is_seeking {
                        let tag_packet = PacketData::Tag {
                            update_type: TagUpdate::State,
                            is_it: is_seeking,
                            minutes: 0,
                            seconds: 0,
                        };
                        let packet = Packet::new(Guid::default(), tag_packet);
                        self.send_players(&players, &ClientCommand::SelfAddressed(packet))
                            .await;
                    }
                    "Updated tag status".to_string()
                }
                PlayerCommand::SendShine { id } => {
                    let shine_packet = PacketData::Shine {
                        shine_id: id,
                        is_grand: false,
                    };
                    let packet = Packet::new(Guid::default(), shine_packet);
                    self.send_players(&players, &ClientCommand::SelfAddressed(packet))
                        .await?;
                    "Sent player shine".to_string()
                }
            },
            ExternalCommand::Shine { command } => match command {
                ShineCommand::Sync => {
                    self.sync_all_shines().await?;
                    format!("Synced shine bags")
                }
                ShineCommand::Clear => {
                    self.lobby.shines.write().await.clear();
                    let players = &self.lobby.players;
                    for mut player in players.iter_mut() {
                        player.value_mut().shine_sync.clear();
                    }
                    format!("Shines cleared")
                }
            },
        };
        Ok(out_str)
    }

    async fn merge_scenario(&self, packet: &Packet) -> Result<()> {
        tracing::debug!("Merging scenario");
        self.cli_broadcast
            .send(ClientCommand::SelfAddressed(packet.clone()))?;
        Ok(())
    }

    async fn persist_shines(&self) {
        let settings = self.lobby.settings.read().await;
        if settings.persist_shines.enabled {
            let filename = settings.persist_shines.filename.clone();
            let shines = self.lobby.shines.clone();
            tokio::spawn(async move {
                let result = save_shines(filename, shines).await;
                if let Err(e) = result {
                    tracing::error!("Error saving shines: {}", e);
                }
            });
        }
    }

    async fn send_players(&self, players: &Players, cmd: &ClientCommand) -> Result<()> {
        match players {
            Players::All => self.broadcast(cmd)?,
            Players::Individual(p) => {
                for guid in p {
                    let cli_ref = self.lobby.get_client(guid)?;
                    let cli = &cli_ref.value().channel;

                    cli.send(cmd.clone()).await?;
                }
            }
        }
        Ok(())
    }

    async fn add_client(&mut self, cmd: ServerCommand) -> Result<()> {
        let (cli, packet, data, comm) = match cmd {
            ServerCommand::NewPlayer {
                cli,
                connect_packet,
                data,
                comm,
            } => (cli, connect_packet, data, comm),
            _ => unreachable!(),
        };

        let (_connection_type, client_name) = match &packet.data {
            PacketData::Connect {
                c_type,
                client_name,
                ..
            } => (c_type, client_name),
            _ => unreachable!(),
        };
        let id = cli.guid;

        let mut names = self.lobby.names.0.write().await;
        names.insert(id, client_name.clone());
        self.lobby.players.insert(id, data);
        drop(names);

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

    async fn setup_player(&mut self, comm: ClientChannel, packet: Packet) -> Result<()> {
        tracing::debug!(
            "Setting up player ({}) with {} other players",
            packet.id,
            self.lobby.players.len() - 1,
        );
        let settings = self.lobby.settings.read().await;
        let max_player = settings.server.max_players;

        drop(settings);
        // Sync connection, costumes, and last game packet
        for other_ref in self.lobby.players.iter() {
            let other_id = other_ref.key();
            let other_cli = other_ref.value();

            let connect_packet = Packet::new(
                *other_id,
                PacketData::Connect {
                    c_type: ConnectionType::FirstConnection,
                    max_player,
                    client_name: other_cli.name.clone(),
                },
            );

            let costume_packet = match &other_cli.costume {
                Some(costume) => Some(Packet::new(*other_id, PacketData::Costume(costume.clone()))),
                _ => None,
            };

            let last_game_packet = other_cli.last_game_packet.clone();

            drop(other_cli);

            comm.send(ClientCommand::Packet(connect_packet)).await?;

            if let Some(p) = costume_packet {
                comm.send(ClientCommand::Packet(p)).await?;
            }

            if let Some(p) = last_game_packet {
                comm.send(ClientCommand::Packet(p)).await?;
            }
        }

        self.broadcast(&ClientCommand::Packet(packet))
    }

    async fn disconnect_player(&mut self, guid: Guid) -> Result<()> {
        tracing::info!("Disconnecting player {}", guid);
        if let Some((guid, data)) = self.lobby.players.remove(&guid) {
            // let name = &data.read().await.name;
            self.lobby.names.0.write().await.remove_by_left(&guid);
            let packet = Packet::new(guid, PacketData::Disconnect);
            self.broadcast(&ClientCommand::Packet(packet.clone()))?;
            let disconnect = ClientCommand::Packet(packet);
            data.channel.send(disconnect).await?;
        }

        Ok(())
    }

    async fn sync_all_shines(&mut self) -> Result<()> {
        let settings = self.lobby.settings.read().await;
        if !settings.shines.enabled {
            return Ok(());
        }

        for player_ref in self.lobby.players.iter() {
            let data = player_ref.value();
            let shines = &data.shine_sync;
            let sender_guid = Guid::default();
            if data.speedrun_start {
                continue;
            }

            client_sync_shines(
                data.channel.clone(),
                self.lobby.shines.clone(),
                &sender_guid,
                &shines,
            )
            .await?;
        }
        Ok(())
    }

    fn broadcast(&self, cmd: &ClientCommand) -> Result<()> {
        self.cli_broadcast.send(cmd.clone())?;
        Ok(())
    }

    async fn shutdown(mut self) {
        let guids: Vec<_> = self.lobby.players.iter().map(|x| *x.key()).collect();
        for guid in guids {
            let _ = self.disconnect_player(guid).await;
        }
    }
}

async fn client_sync_shines(
    to_client: ClientChannel,
    shine_bag: SyncShineBag,
    guid: &Guid,
    client_shines: &ShineBag,
) -> Result<()> {
    // let client = player.read().await;
    let server_shines = shine_bag.read().await;
    let mismatch = server_shines.difference(&client_shines);

    for shine_id in mismatch {
        to_client
            .send(ClientCommand::SelfAddressed(Packet::new(
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

pub fn unalias_map(alias: &str) -> Option<String> {
    let unalias = match alias {
        "cap" => "CapWorldHomeStage",
        "cascade" => "WaterfallWorldHomeStage",
        "sand" => "SandWorldHomeStage",
        "lake" => "LakeWorldHomeStage",
        "wooded" => "ForestWorldHomeStage",
        "cloud" => "CloudWorldHomeStage",
        "lost" => "ClashWorldHomeStage",
        "metro" => "CityWorldHomeStage",
        "sea" => "SeaWorldHomeStage",
        "snow" => "SnowWorldHomeStage",
        "lunch" => "LavaWorldHomeStage",
        "ruined" => "BossRaidWorldHomeStage",
        "bowser" => "SkyWorldHomeStage",
        "moon" => "MoonWorldHomeStage",
        "mush" => "PeachWorldHomeStage",
        "dark" => "Special1WorldHomeStage",
        "darker" => "Special2WorldHomeStage",
        _ => return None,
    };

    Some(unalias.to_string())
}

async fn save_shines(filename: String, shines: SyncShineBag) -> Result<()> {
    let shines = shines.read().await;
    let json_str = serde_json::to_string(&shines.clone())?;
    let mut file = File::open(filename).await?;
    file.write_all(json_str.as_bytes()).await?;

    Ok(())
}

pub fn load_shines(filename: &str) -> Result<ShineBag> {
    let file = std::fs::File::open(filename)?;
    let shines = serde_json::from_reader(file)?;

    Ok(shines)
}
