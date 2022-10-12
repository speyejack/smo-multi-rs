use crate::{
    client::SyncPlayer,
    cmds::{console::ScenarioCommand, ClientCommand, Command, ConsoleCommand, ServerCommand},
    guid::Guid,
    net::{ConnectionType, Packet, PacketData},
    settings::{load_settings, save_settings, SyncSettings},
    types::{ClientInitError, Result, SMOError},
};

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{info_span, Instrument};

pub type SyncClientNames = Arc<RwLock<HashMap<String, Guid>>>;
type SyncShineBag = Arc<RwLock<HashSet<i32>>>;
type ClientChannel = mpsc::Sender<ClientCommand>;

pub struct Coordinator {
    pub shine_bag: SyncShineBag,
    pub settings: SyncSettings,
    pub clients: HashMap<Guid, PlayerInfo>,
    pub client_names: SyncClientNames,
    pub from_clients: mpsc::Receiver<Command>,
    pub cli_broadcast: broadcast::Sender<ClientCommand>,
}

#[derive(Clone)]
pub struct PlayerInfo {
    channel: ClientChannel,
    data: SyncPlayer,
}

pub enum PlayerSelect<T> {
    AllPlayers,
    SelectPlayers(Vec<T>),
}

impl Coordinator {
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
                        if stage == "CapWorldHomeStage" && *scenario_num == 0 {
                            let client = self.get_client(&packet.id)?;
                            let mut data = client.write().await;
                            tracing::info!("Player '{}' starting speedrun", data.name);
                            data.speedrun_start = true;
                            data.shine_sync.clear();
                            drop(data);
                            self.shine_bag.write().await.clear();
                            self.persist_shines().await;
                        } else if stage == "WaterfallWordHomeStage" {
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
                self.broadcast(packet)?;
            }
            Command::Console(cmd, reply) => {
                let result = self.handle_console_cmd(cmd).await;
                reply.send(result).expect("Reply channel failed")
            }
        }
        Ok(true)
    }

    async fn handle_console_cmd(&mut self, cmd: ConsoleCommand) -> Result<String> {
        let string: String = match cmd {
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
                let stage = unalias_map(&stage);
                let data = PacketData::ChangeStage {
                    stage: stage.clone(),
                    id,
                    scenario,
                    sub_scenario: 0,
                };
                let packet = Packet::new(Guid::default(), data);

                let cmd = ClientCommand::SelfAddressed(packet);
                let players = self.get_clients(players.into()).await?;
                self.send_players(players, cmd).await?;
                format!("Sent players to {}:{}", stage, scenario)
            }
            ConsoleCommand::List => {
                let mut player_data = Vec::default();
                for (guid, data) in self.clients.iter() {
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
                let data = PacketData::ChangeStage {
                    id: "$among$us/SubArea".to_string(),
                    stage: "$agogusStage".to_string(),
                    scenario: 21,
                    sub_scenario: 69, // invalid id
                };
                let packet = Packet::new(Guid::default(), data);
                let cmd = ClientCommand::SelfAddressed(packet);
                let players = self.get_clients(players.into()).await?;
                self.send_players(players, cmd).await?;
                "Crashed players".to_string()
            }
            ConsoleCommand::Ban { players } => {
                let players = players.into();
                self.disconnect_players(&players).await;

                let players = self.players_to_guids(players).await?;
                let mut settings = self.settings.write().await;

                let banned_players = settings
                    .ban_list
                    .players
                    .union(&players.into_iter().collect())
                    .copied()
                    .collect();

                settings.ban_list.players = banned_players;

                "Banned players".to_string()
            }
            ConsoleCommand::Rejoin { players } => {
                self.disconnect_players(&players.into()).await;
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
            ConsoleCommand::Tag(tag) => todo!(),
            ConsoleCommand::MaxPlayers { player_count } => {
                let mut settings = self.settings.write().await;
                settings.server.max_players = player_count;
                save_settings(&settings);
                drop(settings);
                self.disconnect_players(&PlayerSelect::AllPlayers).await;
                format!("Saved and set max players to {}", player_count)
            }
            ConsoleCommand::Flip(_) => todo!(),
            ConsoleCommand::Shine(_) => todo!(),
            ConsoleCommand::LoadSettings => {
                let mut settings = self.settings.write().await;
                let new_settings = load_settings()?;
                *settings = new_settings;
                format!("Loaded settings.json")
            }
        };
        Ok(string)
    }

    async fn merge_scenario(&self, packet: &Packet) -> Result<()> {
        self.cli_broadcast
            .send(ClientCommand::SelfAddressed(packet.clone()))?;
        Ok(())
    }

    async fn persist_shines(&self) {
        // TODO
        tracing::warn!("Shine persisting not avaliable.")
    }

    fn get_client_info(&self, id: &Guid) -> std::result::Result<&PlayerInfo, SMOError> {
        self.clients.get(id).ok_or(SMOError::InvalidID(*id))
    }

    fn get_client(&self, id: &Guid) -> std::result::Result<&SyncPlayer, SMOError> {
        self.get_client_info(id).map(|x| &x.data)
    }

    fn get_channel(&self, id: &Guid) -> std::result::Result<&ClientChannel, SMOError> {
        self.get_client_info(id).map(|x| &x.channel)
    }

    async fn players_to_guids(&self, players: PlayerSelect<String>) -> Result<Vec<Guid>> {
        let client_names = self.client_names.read().await;

        let select: Result<Vec<Guid>> = match players {
            PlayerSelect::AllPlayers => Ok(self.clients.keys().copied().collect()),
            PlayerSelect::SelectPlayers(players) => players
                .into_iter()
                .map(|name| {
                    client_names
                        .get(&name)
                        .copied()
                        .ok_or(SMOError::InvalidName(name))
                })
                .collect::<Result<Vec<_>>>(),
        };
        select
    }

    async fn get_clients(
        &self,
        players: PlayerSelect<String>,
    ) -> Result<PlayerSelect<&PlayerInfo>> {
        let client_names = self.client_names.read().await;

        let select = match players {
            PlayerSelect::AllPlayers => PlayerSelect::AllPlayers,
            PlayerSelect::SelectPlayers(players) => PlayerSelect::SelectPlayers(
                players
                    .into_iter()
                    .map(|name| client_names.get(&name).ok_or(SMOError::InvalidName(name)))
                    .map(|guid| {
                        let guid = guid?;
                        self.clients.get(guid).ok_or(SMOError::InvalidID(*guid))
                    })
                    .collect::<Result<_>>()?,
            ),
        };
        Ok(select)
    }

    async fn send_players(
        &self,
        players: PlayerSelect<&PlayerInfo>,
        cmd: ClientCommand,
    ) -> Result<()> {
        match players {
            PlayerSelect::AllPlayers => {
                self.cli_broadcast.send(cmd)?;
            }
            PlayerSelect::SelectPlayers(players) => {
                for p in players {
                    let cli = &p.channel;
                    cli.send(cmd.clone()).await?;
                }
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

        self.client_names.write().await.insert(cli_name, cli_guid);
        self.clients.insert(
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
            self.clients.len() - 1,
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
        ) in self.clients.iter()
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

            let costume_packet =
                Packet::new(*other_id, PacketData::Costume(other_cli.costume.clone()));

            let last_game_packet = other_cli.last_game_packet.clone();

            drop(other_cli);

            comm.send(ClientCommand::Packet(connect_packet)).await?;
            comm.send(ClientCommand::Packet(costume_packet)).await?;

            if let Some(p) = last_game_packet {
                comm.send(ClientCommand::Packet(p)).await?;
            }
        }

        self.broadcast(packet)
    }

    async fn disconnect_players(&mut self, players: &PlayerSelect<String>) {
        let players = match players {
            PlayerSelect::AllPlayers => todo!(),
            PlayerSelect::SelectPlayers(_) => todo!(),
        };
        todo!()
    }

    async fn disconnect_player(&mut self, guid: Guid) -> Result<()> {
        tracing::info!("Disconnecting player {}", guid);
        if let Some(PlayerInfo {
            data,
            channel: comm,
        }) = self.clients.remove(&guid)
        {
            let name = &data.read().await.name;
            self.client_names.write().await.remove(name);
            let packet = Packet::new(guid, PacketData::Disconnect);
            self.broadcast(packet.clone())?;
            let disconnect = ClientCommand::Packet(packet);
            comm.send(disconnect).await?;
        }

        Ok(())
    }

    async fn sync_all_shines(&mut self) -> Result<()> {
        for (
            _guid,
            PlayerInfo {
                channel,
                data: player,
            },
        ) in &self.clients
        {
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

    fn broadcast(&mut self, mut p: Packet) -> Result<()> {
        p.resize();
        self.cli_broadcast.send(ClientCommand::Packet(p.clone()))?;
        // for (cli, _) in &mut self.clients.values() {
        //     cli.send(ClientCommand::Packet(p.clone())).await?;
        // }
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
            .send(ClientCommand::Packet(Packet::new(
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
