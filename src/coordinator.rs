use crate::{
    cmds::{
        ClientCommand, Command, ExternalCommand, PlayerCommand, Players, ServerCommand,
        ShineCommand,
    },
    guid::Guid,
    lobby::{Lobby, LobbyView},
    net::{ConnectionType, GameMode, Packet, PacketData, TagUpdate},
    player_holder::ClientChannel,
    types::Result,
};

use std::{collections::BTreeSet, sync::Arc, time::Duration};
use tokio::{
    fs::File,
    io::AsyncWriteExt,
    sync::{broadcast, mpsc, oneshot, RwLock},
};
use tracing::{info_span, Instrument};

pub type SyncShineBag = Arc<RwLock<ShineBag>>;
pub type ShineBag = BTreeSet<i32>;

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
                        let settings = self.lobby.settings.read().await;
                        let is_excluded = settings.shines.excluded.contains(shine_id);
                        drop(settings);

                        if is_excluded {
                            tracing::info!("Got moon {shine_id} (excluded)");
                        } else {
                            self.lobby.shines.write().await.insert(*shine_id);
                            tracing::info!("Got moon {shine_id}");
                            self.sync_all_shines().await?;
                        }

                        return Ok(true);
                    }
                    PacketData::Game {
                        is_2d: _,
                        scenario_num,
                        stage,
                    } => {
                        tracing::debug!("Got game packet {}->{}", stage, scenario_num);

                        // entering a banned stage?
                        let settings = self.lobby.settings.read().await;
                        let is_stage_banned = settings.ban_list.enabled && settings.ban_list.stages.contains(stage);
                        drop(settings);
                        if is_stage_banned {
                            tracing::warn!("Crashing player for entering banned stage {}.", stage);
                            // crash player in 500ms
                            tokio::spawn({
                                let to_coord = self.lobby.to_coord.clone();
                                async move {
                                    tokio::time::sleep(Duration::from_millis(500)).await;
                                    let (sender, recv) = oneshot::channel();
                                    let _ = to_coord.send(
                                        Command::External(
                                            ExternalCommand::Player {
                                                players : Players::Individual(vec![packet.id]),
                                                command : PlayerCommand::Crash {},
                                            },
                                            sender
                                        )
                                    ).await;
                                    recv.await
                                }
                            });
                            return Ok(true);
                        }

                        // player is on a new save file before entering Cascade kingdom
                        let is_shine_sync_disabled = self.lobby.get_client(&packet.id)?.disable_shine_sync;
                        if (stage == "CapWorldHomeStage" || stage == "CapWorldTowerStage") && *scenario_num == 1 {
                            if !is_shine_sync_disabled {
                                // disable shine sync and clear collected shines for this player
                                let mut player = self.lobby.get_mut_client(&packet.id)?;
                                tracing::info!("Player '{}' entered Cap on new save, preventing moon sync until Cascade", player.name);
                                player.value_mut().disable_shine_sync = true;
                                player.value_mut().shine_sync.clear();
                                drop(player);

                                // clear collected shines remembered by the server
                                let clear_on_new_saves = self.lobby.settings.read().await.shines.clear_on_new_saves;
                                if clear_on_new_saves {
                                    self.lobby.shines.write().await.clear();
                                    self.persist_shines().await;
                                    tracing::info!("Cleared server memory of collected moons");
                                }
                            }
                        } else if is_shine_sync_disabled {
                            tracing::info!("Player {} entered Cascade or later with moon sync disabled, enabling moon sync again", self.lobby.get_client(&packet.id)?.name);
                            let mut lobby = LobbyView::new(&self.lobby);
                            tokio::spawn(async move {
                                // sleep to prevent sending it too early (just a safety measure that is likely not necessary)
                                tokio::time::sleep(Duration::from_millis(2000)).await;
                                // enable shine sync again for this player
                                lobby.get_mut_client(&packet.id)?.value_mut().disable_shine_sync = false;
                                // sync shines to player
                                let shine_sync_enabled = lobby.get_lobby().settings.read().await.shines.enabled;
                                if shine_sync_enabled {
                                    let server_shines = lobby.get_lobby().shines.clone();

                                    let player = lobby.get_lobby().get_client(&packet.id)?;
                                    let player_channel = player.channel.clone();
                                    let excluded_shines = &lobby.get_lobby().settings.read().await.shines.excluded;
                                    let player_shines = player.shine_sync.union(&excluded_shines).copied().collect();
                                    drop(player);

                                    let result = client_sync_shines(
                                        player_channel,
                                        server_shines,
                                        &packet.id,
                                        &player_shines,
                                    )
                                    .await;
                                    if let Err(e) = result {
                                        tracing::warn!("Initial shine sync failed: {e}")
                                    }
                                }
                                Ok(()) as Result<()>
                            });
                        }
                        tracing::debug!("Changing scenarios: {} {}", scenario_num, stage);

                        let merge_scenario =
                            self.lobby.settings.read().await.scenario.merge_enabled;
                        if merge_scenario {
                            self.merge_scenario(&packet).await?;
                        }
                    }
                    PacketData::Tag { game_mode, .. } | PacketData::GameMode { game_mode, .. } => {
                        // entering a banned gamemode?
                        let settings = self.lobby.settings.read().await;
                        let is_gamemode_banned = settings.ban_list.enabled && settings.ban_list.game_modes.contains(&GameMode::to_i8(*game_mode));
                        drop(settings);
                        if is_gamemode_banned {
                            tracing::warn!("Crashing player for entering banned game mode {}.", game_mode);
                            // crash player in 500ms
                            tokio::spawn({
                                let to_coord = self.lobby.to_coord.clone();
                                async move {
                                    tokio::time::sleep(Duration::from_millis(500)).await;
                                    let (sender, recv) = oneshot::channel();
                                    let _ = to_coord.send(
                                        Command::External(
                                            ExternalCommand::Player {
                                                players : Players::Individual(vec![packet.id]),
                                                command : PlayerCommand::Crash {},
                                            },
                                            sender
                                        )
                                    ).await;
                                    recv.await
                                }
                            });
                            return Ok(true);
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
                        id           : "$among$us/cr4sh%".to_string(),
                        stage        : "$agogusStage".to_string(),
                        scenario     : 21,
                        sub_scenario : 69, // invalid id
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
                            game_mode: GameMode::Legacy,
                            update_type: TagUpdate::Time,
                            is_it: false,
                            minutes,
                            seconds,
                        };
                        let packet = Packet::new(Guid::default(), tag_packet);
                        self.send_players(&players, &ClientCommand::SelfAddressed(packet)).await?;
                    }

                    if let Some(is_seeking) = is_seeking {
                        let tag_packet = PacketData::Tag {
                            game_mode: GameMode::Legacy,
                            update_type: TagUpdate::State,
                            is_it: is_seeking,
                            minutes: 0,
                            seconds: 0,
                        };
                        let packet = Packet::new(Guid::default(), tag_packet);
                        self.send_players(&players, &ClientCommand::SelfAddressed(packet)).await?;
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

        let client_name = match &packet.data {
            PacketData::Connect {
                client_name,
                ..
            } => client_name,
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

        // Sync other players to the new player
        for other_ref in self.lobby.players.iter() {
            let other_id  = other_ref.key();
            let other_cli = other_ref.value();

            let connect_packet = Packet::new(
                *other_id,
                PacketData::Connect {
                    c_type: ConnectionType::FirstConnection,
                    max_player,
                    client_name: other_cli.name.clone(),
                },
            );

            let packets = [
                Some(connect_packet),
                other_cli.last_costume_packet.clone(),
                other_cli.last_capture_packet.clone(),
                other_cli.create_tag_packet(*other_id),
                other_cli.last_game_packet.clone(),
                other_cli.last_player_packet.clone(),
            ];

            for packet in packets {
                if let Some(p) = packet {
                    comm.send(ClientCommand::Packet(p)).await?;
                }
            }
        }

        let client_id = packet.id;
        let conn_type = match packet.data {
            PacketData::Connect {
                c_type,
                ..
            } => c_type,
            _ => unreachable!(),
        };

        // Sync new player to other players
        self.broadcast(&ClientCommand::Packet(packet))?;

        // make the other clients reset their puppet cache for this client, if it is a new connection (after restart)
        if conn_type == ConnectionType::FirstConnection {
            // empty tag packet
            self.broadcast(&ClientCommand::Packet(Packet::new(
                client_id,
                PacketData::Tag {
                    game_mode   : GameMode::Legacy,
                    update_type : TagUpdate::Both,
                    is_it       : false,
                    seconds     : 0,
                    minutes     : 0,
                },
            )))?;
            // empty capture packet
            self.broadcast(&ClientCommand::Packet(Packet::new(
                client_id,
                PacketData::Capture {
                    model: "".to_string(),
                },
            )))?;
        }

        Ok(())
    }

    async fn disconnect_player(&mut self, guid: Guid) -> Result<()> {
        tracing::info!("Disconnecting player {}", guid);
        // TODO: do not remove the player, but mark it as disconnected, so that
        // after a reconnect its packets are still there to send to new players.
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

        let excluded_shines = &settings.shines.excluded;

        for player_ref in self.lobby.players.iter() {
            let player = player_ref.value();
            let player_shines = player.shine_sync.union(&excluded_shines).copied().collect();
            let server_shines = self.lobby.shines.clone();
            let sender_guid = Guid::default();

            if player.disable_shine_sync {
                continue;
            }

            client_sync_shines(
                player.channel.clone(),
                server_shines,
                &sender_guid,
                &player_shines,
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
