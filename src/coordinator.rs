use crate::{
    client::SyncPlayer,
    cmds::{
        console::{FlipCommand, ScenarioCommand, ShineCommand, TagCommand, UdpCommand},
        ClientCommand, Command, ConsoleCommand, ServerCommand, ServerWideCommand,
    },
    guid::Guid,
    net::{ConnectionType, Packet, PacketData, TagUpdate},
    player_holder::{ClientChannel, PlayerHolder, PlayerInfo, PlayerSelect},
    settings::{load_settings, save_settings, SyncSettings},
    types::{ClientInitError, Result, SMOError},
};

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};
use tokio::{
    fs::File,
    io::AsyncWriteExt,
    sync::{broadcast, mpsc, RwLock},
    time,
};
use tracing::{info_span, Instrument};

pub type SyncClientNames = Arc<RwLock<HashMap<String, Guid>>>;
pub type SyncShineBag = Arc<RwLock<ShineBag>>;
pub type ShineBag = HashSet<i32>;

pub struct Coordinator {
    pub shine_bag: SyncShineBag,
    pub settings: SyncSettings,
    pub from_clients: mpsc::Receiver<Command>,
    pub cli_broadcast: broadcast::Sender<ClientCommand>,
    pub server_broadcast: broadcast::Sender<ServerWideCommand>,
    players: PlayerHolder,
}

impl Coordinator {
    pub fn new(
        settings: SyncSettings,
        from_clients: mpsc::Receiver<Command>,
        cli_broadcast: broadcast::Sender<ClientCommand>,
        server_broadcast: broadcast::Sender<ServerWideCommand>,
        shine_bag: ShineBag,
    ) -> Self {
        Coordinator {
            settings,
            from_clients,
            cli_broadcast,
            server_broadcast,
            shine_bag: Arc::new(RwLock::new(shine_bag)),
            players: Default::default(),
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
                        tracing::debug!("Got game packet {}->{}", stage, scenario_num);
                        let client = self.players.get_client(&packet.id)?;
                        if stage == "CapWorldHomeStage" && *scenario_num == 0 {
                            let mut data = client.write().await;
                            tracing::debug!("Player '{}' started new save", data.name);
                            data.speedrun_start = true;
                            data.shine_sync.clear();
                            drop(data);
                            self.shine_bag.write().await.clear();
                            self.persist_shines().await;
                        } else if stage == "WaterfallWordHomeStage" {
                            let mut data = client.write().await;
                            tracing::debug!("Enabling shine sync for player '{}'", data.name);
                            let was_speed_run = data.speedrun_start;
                            data.speedrun_start = false;
                            drop(data);
                            let should_sync_shines = self.settings.read().await.shines.enabled;

                            if should_sync_shines && was_speed_run {
                                let client = client.clone();
                                let channel = self.players.get_channel(&packet.id)?.clone();
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
                        }
                        tracing::debug!("Changing scenarios: {} {}", scenario_num, stage);

                        let merge_scenario = self.settings.read().await.scenario.merge_enabled;
                        if merge_scenario {
                            self.merge_scenario(&packet).await?;
                        }
                    }
                    _ => {}
                };
                self.broadcast(packet)?;
            }
            Command::Console(cmd, reply) => {
                let result = self.handle_console_cmd(cmd).await;

                if let Err(SMOError::ServerShutdown) = result {
                    self.server_broadcast.send(ServerWideCommand::Shutdown)?;
                    return Ok(false);
                }

                reply.send(result).expect("Reply channel failed")
            }
        }
        Ok(true)
    }

    async fn handle_console_cmd(&mut self, cmd: ConsoleCommand) -> Result<String> {
        let string: String = match cmd {
            ConsoleCommand::Restart => return Err(SMOError::ServerShutdown),
            ConsoleCommand::SendAll { stage } => {
                let stage = unalias_map(&stage);
                let data = PacketData::ChangeStage {
                    stage: stage.clone(),
                    id: "".to_string(),
                    scenario: -1,
                    sub_scenario: 0,
                };
                let p = Packet::new(Guid::default(), data);
                self.cli_broadcast.send(ClientCommand::SelfAddressed(p))?;
                format!("Sent players to {}:-1", stage)
            }
            ConsoleCommand::Send {
                stage,
                id,
                scenario,
                players,
            } => {
                if players.is_empty() {
                    return Err(SMOError::InvalidConsoleArg("Players empty".to_string()));
                }

                let stage = unalias_map(&stage);
                let data = PacketData::ChangeStage {
                    stage: stage.clone(),
                    id,
                    scenario,
                    sub_scenario: 0,
                };
                let packet = Packet::new(Guid::default(), data);

                let cmd = ClientCommand::SelfAddressed(packet);
                let players = self.players.get_clients(&players[..].into()).await?;
                self.send_players(&players, &cmd).await?;
                format!("Sent players to {}:{}", stage, scenario)
            }
            ConsoleCommand::List => {
                let mut player_data = Vec::default();
                for (guid, data) in self.players.clients.iter() {
                    let name = &data.data.read().await.name;
                    player_data.push((guid, name.to_string()));
                }

                let player_strs: Vec<String> = player_data
                    .into_iter()
                    .map(|x| format!("{} ({})", x.0, x.1))
                    .collect();

                format!("List: \n\t{}", player_strs.join("\n\t"))
            }
            ConsoleCommand::Crash { players } => {
                if players.is_empty() {
                    return Err(SMOError::InvalidConsoleArg("Players empty".to_string()));
                }

                let data = PacketData::ChangeStage {
                    id: "$among$us/SubArea".to_string(),
                    stage: "$agogusStage".to_string(),
                    scenario: 21,
                    sub_scenario: 69, // invalid id
                };
                let packet = Packet::new(Guid::default(), data);
                let cmd = ClientCommand::SelfAddressed(packet);
                let players = self.players.get_clients(&players[..].into()).await?;
                self.send_players(&players, &cmd).await?;
                "Crashed players".to_string()
            }
            ConsoleCommand::Ban { players } => {
                if players.is_empty() {
                    return Err(SMOError::InvalidConsoleArg("Players empty".to_string()));
                }

                let players = players[..].into();
                self.disconnect_players(&players).await?;

                let player_data = self.players.get_clients(&players).await?;
                let player_data = self.players.flatten_players(&player_data).await;

                let mut ips = Vec::new();
                for player in player_data {
                    let data = player.data.read().await;
                    if let Some(x) = data.ipv4 {
                        ips.push(x)
                    }
                }

                let players = self.players.players_to_guids(&players).await?;
                let mut settings = self.settings.write().await;

                let banned_players = settings
                    .ban_list
                    .players
                    .union(&players.into_iter().collect())
                    .copied()
                    .collect();

                settings.ban_list.players = banned_players;

                let ips = settings
                    .ban_list
                    .ip_addresses
                    .union(&ips.into_iter().collect())
                    .copied()
                    .collect();
                settings.ban_list.ip_addresses = ips;

                "Banned players".to_string()
            }
            ConsoleCommand::Rejoin { players } => {
                if players.is_empty() {
                    return Err(SMOError::InvalidConsoleArg("Players empty".to_string()));
                }

                self.disconnect_players(&players[..].into()).await?;
                "Rejoined players".to_string()
            }
            ConsoleCommand::Scenario(scenario) => match scenario {
                ScenarioCommand::Merge { enabled } => match enabled {
                    Some(to_enabled) => {
                        let mut settings = self.settings.write().await;
                        settings.scenario.merge_enabled = to_enabled;
                        save_settings(&settings)?;
                        drop(settings);
                        if to_enabled {
                            "Enabled scenario merge"
                        } else {
                            "Disabled scenario merge"
                        }
                        .to_string()
                    }
                    None => {
                        let is_enabled = self.settings.read().await.scenario.merge_enabled;
                        format!("Scenario merging is {}", is_enabled)
                    }
                },
            },
            ConsoleCommand::Tag(tag) => match tag {
                TagCommand::Time {
                    player,
                    minutes,
                    seconds,
                } => {
                    if seconds >= 60 {
                        return Err(SMOError::InvalidConsoleArg(
                            "Invalid number of seconds".to_string(),
                        ));
                    }

                    let players = self.players.get_clients(&([player][..]).into()).await?;

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
                    format!("Set time for players to {}:{}", minutes, seconds)
                }
                TagCommand::Seeking { player, is_seeking } => {
                    let players = self.players.get_clients(&([player][..]).into()).await?;
                    // TODO test if times are the correct default
                    let tag_packet = PacketData::Tag {
                        update_type: TagUpdate::State,
                        is_it: is_seeking,
                        minutes: 0,
                        seconds: 0,
                    };
                    let packet = Packet::new(Guid::default(), tag_packet);

                    self.send_players(&players, &ClientCommand::SelfAddressed(packet))
                        .await?;
                    format!("Changed is_seeking state to {}", is_seeking)
                }
                TagCommand::Start { countdown, seekers } => {
                    let seeker_ids = seekers[..].into();
                    let seekers = self.players.get_clients(&seeker_ids).await?;
                    let hiders = self.players.get_clients(&!seeker_ids).await?;

                    // TODO make this a separate task
                    time::sleep(Duration::from_secs(countdown.into())).await;

                    let seeker_packet = PacketData::Tag {
                        update_type: TagUpdate::State,
                        is_it: true,
                        seconds: 0,
                        minutes: 0,
                    };
                    let seeker_packet =
                        ClientCommand::SelfAddressed(Packet::new(Guid::default(), seeker_packet));

                    let hider_packet = PacketData::Tag {
                        update_type: TagUpdate::State,
                        is_it: false,
                        seconds: 0,
                        minutes: 0,
                    };
                    let hider_packet =
                        ClientCommand::SelfAddressed(Packet::new(Guid::default(), hider_packet));

                    self.send_players(&seekers, &seeker_packet).await?;
                    self.send_players(&hiders, &hider_packet).await?;
                    "Started game after {countdown} seconds.".to_string()
                }
            },
            ConsoleCommand::MaxPlayers { player_count } => {
                let mut settings = self.settings.write().await;
                settings.server.max_players = player_count;
                save_settings(&settings)?;
                drop(settings);
                self.disconnect_players(&PlayerSelect::AllPlayers).await?;
                format!("Saved and set max players to {}", player_count)
            }

            ConsoleCommand::Flip(flip) => match flip {
                FlipCommand::List => {
                    let settings = self.settings.read().await;
                    let player_str: Vec<String> = settings
                        .flip
                        .players
                        .iter()
                        .map(ToString::to_string)
                        .collect();
                    drop(settings);
                    format!("User ids: {}", &player_str.join(", "))
                }
                FlipCommand::Add { player } => {
                    let mut settings = self.settings.write().await;
                    settings.flip.players.insert(player);
                    save_settings(&settings)?;
                    drop(settings);
                    format!("Added {} to flipped players", player)
                }
                FlipCommand::Remove { player } => {
                    let mut settings = self.settings.write().await;
                    let was_removed = settings.flip.players.remove(&player);
                    save_settings(&settings)?;
                    drop(settings);
                    match was_removed {
                        true => format!("Removed {} to flipped players", player),
                        false => format!("User {} wasn't in the flipped players list", player),
                    }
                }
                FlipCommand::Set { is_flipped } => {
                    let mut settings = self.settings.write().await;
                    settings.flip.enabled = is_flipped;
                    save_settings(&settings)?;
                    if is_flipped {
                        "Enabled player flipping".to_string()
                    } else {
                        "Disabled player flipping".to_string()
                    }
                }
                FlipCommand::Pov { value } => {
                    let mut settings = self.settings.write().await;
                    settings.flip.pov = value;
                    save_settings(&settings)?;
                    format!("Point of view set to {}", value)
                }
            },
            ConsoleCommand::Shine(shine) => match shine {
                ShineCommand::List => {
                    let shines = self.shine_bag.read().await;
                    let str_shines = shines
                        .iter()
                        .map(|x| x.to_string())
                        .collect::<Vec<String>>()
                        .join(", ");
                    str_shines
                }
                ShineCommand::Clear => {
                    self.shine_bag.write().await.clear();
                    for player in self.players.clients.values() {
                        player.data.write().await.shine_sync.clear();
                    }
                    "Cleared shine bags".to_string()
                }
                ShineCommand::Sync => {
                    self.sync_all_shines().await?;
                    "Synced shine bag automatically".to_string()
                }
                ShineCommand::Send { id, player } => {
                    let players = self.players.get_clients(&[player][..].into()).await?;
                    let shine_packet = PacketData::Shine {
                        shine_id: id.try_into().expect("Could not convert shine id"),
                        is_grand: false,
                    };
                    let packet = Packet::new(Guid::default(), shine_packet);
                    self.send_players(&players, &ClientCommand::SelfAddressed(packet))
                        .await?;
                    format!("Send shine num {}", id)
                }
                ShineCommand::Set { should_sync } => {
                    let mut settings = self.settings.write().await;
                    settings.shines.enabled = should_sync;
                    save_settings(&settings)?;

                    if should_sync {
                        "Enabled shine sync".to_string()
                    } else {
                        "Disabled shine sync".to_string()
                    }
                }
            },
            ConsoleCommand::Udp(udpcmd) => match udpcmd {
                UdpCommand::Init { player } => {
                    let players = self.players.get_clients(&[player][..].into()).await?;
                    let p = Packet::new(Guid::default(), PacketData::UdpInit { port: 0 });
                    self.send_players(&players, &ClientCommand::SelfAddressed(p))
                        .await?;
                    format!("Initiated udp handshakes")
                }
                UdpCommand::Auto { should_auto } => {
                    let mut settings = self.settings.write().await;
                    settings.udp.initiate_handshake = should_auto;
                    drop(settings);
                    if should_auto {
                        "Enabled auto udp handshake"
                    } else {
                        "Disabled auto udp handshake"
                    }
                    .to_string()
                }
            },
            ConsoleCommand::LoadSettings => {
                let mut settings = self.settings.write().await;
                let new_settings = load_settings()?;
                *settings = new_settings;
                "Loaded settings.json".to_string()
            }
        };
        Ok(string)
    }

    async fn merge_scenario(&self, packet: &Packet) -> Result<()> {
        tracing::debug!("Merging scenario");
        self.cli_broadcast
            .send(ClientCommand::SelfAddressed(packet.clone()))?;
        Ok(())
    }

    async fn persist_shines(&self) {
        let settings = self.settings.read().await;
        if settings.persist_shines.enabled {
            let filename = settings.persist_shines.filename.clone();
            let shines = self.shine_bag.clone();
            tokio::spawn(async move {
                let result = save_shines(filename, shines).await;
                if let Err(e) = result {
                    tracing::error!("Error saving shines: {}", e);
                }
            });
        }
    }

    async fn send_players(
        &self,
        players: &PlayerSelect<&PlayerInfo>,
        cmd: &ClientCommand,
    ) -> Result<()> {
        match players {
            PlayerSelect::AllPlayers => {
                self.cli_broadcast.send(cmd.clone())?;
            }
            PlayerSelect::SelectPlayers(players) => {
                for p in players {
                    let cli = &p.channel;
                    cli.send(cmd.clone()).await?;
                }
            }
            PlayerSelect::ExcludePlayers(_players) => {
                unimplemented!("Excluded players not available to send to.")
            }
        };
        Ok(())
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

        let (connection_type, _client_name) = match &packet.data {
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
            let banned_ips = &settings.ban_list.ip_addresses;

            if max_players <= self.players.clients.len() {
                tracing::warn!(
                    "Reached max players: {} <= {}",
                    max_players,
                    self.players.clients.len()
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
            ConnectionType::Reconnecting => match self.players.clients.remove(&id) {
                Some(PlayerInfo {
                    data: prev_data, ..
                }) => {
                    cli.player = prev_data.clone();
                    prev_data
                }
                None => cli.player.clone(),
            },
        };

        let cli_name = cli.player.read().await.name.clone();
        let cli_guid = cli.guid;

        self.players
            .names
            .0
            .write()
            .await
            .insert_no_overwrite(cli_guid, cli_name)
            .map_err(|_| SMOError::ClientInit(ClientInitError::DuplicateClient))?;
        self.players.clients.insert(
            id,
            PlayerInfo {
                channel: comm.clone(),
                data,
            },
        );

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
            self.players.clients.len() - 1,
        );
        let settings = self.settings.read().await;
        let max_player = settings.server.max_players;

        drop(settings);
        // Sync connection, costumes, and last game packet
        for (
            other_id,
            PlayerInfo {
                data: other_cli, ..
            },
        ) in self.players.clients.iter()
        {
            let other_cli = other_cli.read().await;

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

        self.broadcast(packet)
    }

    async fn disconnect_players(&mut self, players: &PlayerSelect<String>) -> Result<()> {
        let guids = self.players.players_to_guids(players).await?;
        for guid in guids {
            self.disconnect_player(guid).await?;
        }
        Ok(())
    }

    async fn disconnect_player(&mut self, guid: Guid) -> Result<()> {
        tracing::info!("Disconnecting player {}", guid);
        if let Some(PlayerInfo {
            data,
            channel: comm,
        }) = self.players.clients.remove(&guid)
        {
            let name = &data.read().await.name;
            self.players.names.0.write().await.remove_by_right(name);
            let packet = Packet::new(guid, PacketData::Disconnect);
            self.broadcast(packet.clone())?;
            let disconnect = ClientCommand::Packet(packet);
            comm.send(disconnect).await?;
        }

        Ok(())
    }

    async fn sync_all_shines(&mut self) -> Result<()> {
        let settings = self.settings.read().await;
        if !settings.shines.enabled {
            return Ok(());
        }

        for (
            _guid,
            PlayerInfo {
                channel,
                data: player,
            },
        ) in &self.players.clients
        {
            let sender_guid = Guid::default();
            client_sync_shines(
                channel.clone(),
                self.shine_bag.clone(),
                &sender_guid,
                &player,
            )
            .await?;
        }
        Ok(())
    }

    fn broadcast(&mut self, mut p: Packet) -> Result<()> {
        p.resize();
        self.cli_broadcast.send(ClientCommand::Packet(p.clone()))?;
        Ok(())
    }

    async fn shutdown(mut self) {
        let active_clients = self.players.clients.clone();
        for guid in active_clients.keys() {
            let _ = self.disconnect_player(*guid).await;
        }
    }
}

async fn client_sync_shines(
    to_client: ClientChannel,
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

fn unalias_map(alias: &str) -> String {
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
        s => s,
    };

    unalias.to_string()
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
