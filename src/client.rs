use crate::{
    cmds::{ClientCommand, Command, ServerCommand},
    guid::Guid,
    json_api::JsonApi,
    lobby::{Lobby, LobbyView},
    net::{connection::Connection, udp_conn::UdpConnection, ConnectionType, GameMode, Packet, PacketData, TagUpdate},
    player_holder::ClientChannel,
    types::{ChannelError, ClientInitError, ErrorSeverity, Result, SMOError, Vector3},
};
use dashmap::mapref::one::{Ref, RefMut};
use nalgebra::UnitQuaternion;
use std::{
    collections::{hash_map::RandomState, BTreeSet},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};
use tokio::{
    io::AsyncWriteExt,
    net::{TcpStream, UdpSocket},
    select,
    sync::{broadcast, mpsc},
};
use tracing::Level;

#[derive(Debug)]
pub struct Client {
    pub display_name: String,
    pub guid: Guid,
    pub alive: bool,
    pub conn: Connection,
    pub udp_conn: UdpConnection,
    pub to_coord: mpsc::Sender<Command>,
    pub from_server: mpsc::Receiver<ClientCommand>,
    pub send_broadcast: broadcast::Sender<ClientCommand>,
    pub recv_broadcast: broadcast::Receiver<ClientCommand>,

    lobby: Lobby,
}

#[derive(Clone, Debug)]
pub struct PlayerData {
    pub ipv4: Option<IpAddr>,
    pub name: String,
    pub game_mode: GameMode,
    pub shine_sync: BTreeSet<i32>,
    pub scenario: i8,
    pub is_2d: bool,
    pub is_seeking: Option<bool>,
    pub last_capture_packet: Option<Packet>,
    pub last_costume_packet: Option<Packet>,
    pub last_game_packet: Option<Packet>,
    pub last_player_packet: Option<Packet>,
    pub disable_shine_sync: bool,
    pub loaded_save: bool,
    pub time: Option<Duration>,
    pub channel: ClientChannel,
}

impl PlayerData {
    fn new(channel: ClientChannel) -> Self {
        Self {
            ipv4: Default::default(),
            name: Default::default(),
            game_mode: GameMode::None,
            shine_sync: Default::default(),
            scenario: Default::default(),
            is_2d: Default::default(),
            is_seeking: Default::default(),
            last_capture_packet: Default::default(),
            last_costume_packet: Default::default(),
            last_game_packet: Default::default(),
            last_player_packet: Default::default(),
            disable_shine_sync: Default::default(),
            loaded_save: Default::default(),
            time: Default::default(),
            channel,
        }
    }

    pub fn create_tag_packet(&self, guid: Guid) -> Option<Packet> {
        let update_type = match (self.time, self.is_seeking) {
            (Some(_), Some(_)) => TagUpdate::Both,
            (Some(_), None)    => TagUpdate::Time,
            (None, Some(_))    => TagUpdate::State,
            (None, None)       => TagUpdate::Unknown,
        };
        if update_type == TagUpdate::Unknown {
            return None
        }
        let seconds = match self.time {
          Some(duration) => duration.as_secs(),
          None           => 0,
        };
        Some(Packet::new(
            guid,
            PacketData::Tag {
                game_mode: GameMode::Legacy,
                update_type,
                is_it   : self.is_seeking.unwrap_or(false),
                seconds : u8::try_from(seconds % 60).unwrap_or(59),
                minutes : u16::try_from(seconds / 60).unwrap_or(u16::MAX),
            },
        ))
    }
}

#[derive(Debug)]
enum ClientEvent {
    Incoming(Packet),
    Outgoing(ClientCommand),
}

pub fn get_mario_size(is_2d: bool) -> f32 {
    if is_2d {
        180.0
    } else {
        160.0
    }
}

#[derive(Debug)]
enum PacketDestination {
    NoSend,
    Broadcast,
    Coordinator,
}

impl Client {
    /// Loop over events until an event signals to quit
    pub async fn handle_events(mut self) -> Result<()> {
        while self.alive {
            let event = self.read_event().await;

            let result = match event {
                Ok(ClientEvent::Incoming(p)) => self.handle_packet(p).await,
                Ok(ClientEvent::Outgoing(c)) => self.handle_command(c).await,
                Err(e) => match e.severity() {
                    ErrorSeverity::ClientFatal => {
                        self.alive = false;
                        break;
                    }
                    _ => Err(e),
                },
            };

            if let Err(e) = result {
                tracing::warn!("Error with client {}: {}", self.guid, e)
            }
        }

        self.disconnect().await?;
        Ok(())
    }

    /// Read an event from either the client sockets or server channels
    async fn read_event(&mut self) -> Result<ClientEvent> {
        let event = select! {
            packet = self.conn.read_packet() => {
                ClientEvent::Incoming(packet?)
            },
            udp_packet = self.udp_conn.read_packet() => {
                ClientEvent::Incoming(udp_packet?)
            },
            command = self.from_server.recv() => ClientEvent::Outgoing(command.ok_or(ChannelError::RecvChannel)?),
            command = self.recv_broadcast.recv() => ClientEvent::Outgoing(command?),
        };
        Ok(event)
    }

    /// Disconnect the player
    pub async fn disconnect(mut self) -> Result<()> {
        tracing::warn!("Client {} disconnected", self.display_name);
        self.to_coord
            .send(Command::Server(ServerCommand::DisconnectPlayer {
                guid: self.guid,
            }))
            .await?;
        self.conn.socket.shutdown().await?;
        Ok(())
    }

    /// Handle any incoming packets from the client
    async fn handle_packet(&mut self, mut packet: Packet) -> Result<()> {
        match packet.data {
            PacketData::Player { .. } | PacketData::Cap { .. } => {}
            _ => tracing::trace!("Handling packet: {}", &packet.data.get_type_name()),
        }

        let send_destination = match &mut packet.data {
            PacketData::Player {
                ref mut rot,
                ref mut pos,
                ..
            } => {
                let settings = self.lobby.settings.read().await;
                if settings.flip.enabled
                    && settings.flip.pov.is_others_flip()
                    && settings.flip.players.get(&packet.id).is_some()
                {
                    let angle = std::f32::consts::PI;
                    let rot_quad = *(UnitQuaternion::from_axis_angle(&Vector3::z_axis(), angle));
                    let data = self.get_player();
                    *pos += get_mario_size(data.is_2d) * Vector3::y();
                    *rot *= rot_quad;
                }
                drop(settings);

                let mut data = self.lobby.get_mut_client(&self.guid)?;
                data.last_player_packet = Some(packet.clone());
                drop(data);

                PacketDestination::Coordinator
            }
            PacketData::Capture { .. } => {
                let mut data = self.get_player_mut();
                data.last_capture_packet = Some(packet.clone());
                drop(data);
                PacketDestination::Broadcast
            }
            PacketData::Costume { .. } => {
                let mut data = self.get_player_mut();
                data.loaded_save = true;
                data.last_costume_packet = Some(packet.clone());
                drop(data);
                PacketDestination::Coordinator
            }
            PacketData::Game {
                is_2d,
                scenario_num,
                stage,
            } => {
                let mut data = self.get_player_mut();
                data.is_2d = *is_2d;
                data.scenario = *scenario_num;
                // reset last_player_packet on stage changes
                if let Some(Packet { data: PacketData::Game { stage: last_stage, .. }, .. }) = &data.last_game_packet {
                    if *stage != *last_stage {
                        data.last_player_packet = None;
                    }
                }
                data.last_game_packet = Some(packet.clone());
                drop(data);
                PacketDestination::Coordinator
            }
            PacketData::Tag {
                game_mode,
                update_type,
                is_it,
                seconds,
                minutes,
            } => {
                let mut data = self.get_player_mut();
                match update_type {
                    crate::net::TagUpdate::Time => {
                        data.time = Some(Duration::from_secs(*seconds as u64 + *minutes as u64 * 60));
                    }
                    crate::net::TagUpdate::State => {
                        data.is_seeking = Some(*is_it);
                    }
                    crate::net::TagUpdate::Both => {
                        data.time       = Some(Duration::from_secs(*seconds as u64 + *minutes as u64 * 60));
                        data.is_seeking = Some(*is_it);
                    }
                    _ => {}
                }
                data.game_mode = *game_mode;
                drop(data);
                PacketDestination::Coordinator
            }
            PacketData::GameMode {
                game_mode,
                ..
            } => {
                let mut data = self.get_player_mut();
                data.time       = None;
                data.is_seeking = None;
                data.game_mode  = *game_mode;
                drop(data);
                PacketDestination::Coordinator
            }
            PacketData::Shine { shine_id, .. } => {
                let mut data = self.get_player_mut();
                if data.loaded_save {
                    data.shine_sync.insert(*shine_id);
                }
                drop(data);
                PacketDestination::Coordinator
            }
            PacketData::UdpInit { port } => {
                tracing::debug!(
                    "{} completed udp handshake, attempting hybrid connection",
                    self.display_name
                );
                self.udp_conn.set_client_port(*port);
                // Attempt to send some udp data to client
                let holepunch = Packet::new(self.guid, PacketData::HolePunch);
                self.udp_conn.write_packet(&holepunch).await?;
                PacketDestination::NoSend
            }
            PacketData::HolePunch => PacketDestination::NoSend,
            _ => PacketDestination::Broadcast,
        };

        match send_destination {
            PacketDestination::NoSend => {}
            PacketDestination::Broadcast => {
                let mut packet = packet;
                packet.resize();
                self.send_broadcast.send(ClientCommand::Packet(packet))?;
            }
            PacketDestination::Coordinator => self.to_coord.send(Command::Packet(packet)).await?,
        }

        Ok(())
    }

    /// Handle any commands sent from internal channels
    async fn handle_command(&mut self, command: ClientCommand) -> Result<()> {
        match command {
            ClientCommand::Packet(mut p) => {
                match &mut p.data {
                    // Same pid handling
                    PacketData::Disconnect if p.id == self.guid => {
                        self.alive = false;
                        // Disconnect packets handled later
                        return Ok(());
                    }
                    _ if p.id == self.guid => return Ok(()),
                    // Any different pids
                    PacketData::Player {
                        ref mut pos,
                        ref mut rot,
                        ..
                    } => {
                        let settings = self.lobby.settings.read().await;
                        if settings.flip.enabled
                            && settings.flip.pov.is_self_flip()
                            && settings.flip.players.get(&self.guid).is_some()
                            && settings.flip.players.get(&p.id).is_none()
                        {
                            let angle = std::f32::consts::PI;
                            let rot_quad =
                                *(UnitQuaternion::from_axis_angle(&Vector3::z_axis(), angle));
                            let data = self.get_player();
                            *pos += get_mario_size(data.is_2d) * Vector3::y();
                            *rot *= rot_quad;
                        }
                    }
                    _ => {}
                }
                self.send_packet(&p).await?;
            }
            ClientCommand::SelfAddressed(mut p) => {
                // Update local client data with any outgoing packet data
                match p.data {
                    PacketData::UdpInit { ref mut port } => {
                        let new_port = self
                            .udp_conn
                            .socket
                            .local_addr()
                            .map(|x| x.port())
                            .map_err(|e| {
                                anyhow::anyhow!("Unable to get local udp address: {}", e)
                            })?;
                        *port = new_port;
                    }
                    PacketData::Shine { shine_id, .. } => {
                        let mut data = self.get_player_mut();
                        data.shine_sync.insert(shine_id);
                    }
                    PacketData::Disconnect {} => {
                        // Disconnect packets handled later
                        self.alive = false;
                    }
                    _ => {}
                }

                self.readdress_and_send(&mut p).await?;
            }
        }
        Ok(())
    }

    /// Send packet to player using either tcp or udp
    pub async fn send_packet(&mut self, packet: &Packet) -> Result<()> {
        // Packet logging
        if tracing::enabled!(Level::TRACE) {
            match packet.data {
                PacketData::Player { .. } | PacketData::Cap { .. } => {}
                _ => {
                    tracing::trace!(
                        "Sending packet: {}->{}",
                        packet.id,
                        packet.data.get_type_name()
                    );
                }
            }
        }

        match packet.data {
            // Use UDP traffic for player and cap if possible
            PacketData::Player { .. } | PacketData::Cap { .. } if self.udp_conn.is_client_udp() => {
                self.udp_conn.write_packet(packet).await
            }
            // Fallback to tcp otherwise
            _ => self.conn.write_packet(packet).await,
        }
    }

    /// Readdress packet to come from the same guid as client then send
    pub async fn readdress_and_send(&mut self, p: &mut Packet) -> Result<()> {
        p.id = self.guid;
        self.send_packet(p).await
    }

    /// Perform the initialization and handshake with client then hand off to coordinator
    pub async fn initialize_client(
        socket: TcpStream,
        to_coord: mpsc::Sender<Command>,
        broadcast: broadcast::Sender<ClientCommand>,
        udp_port: u16,
        lobby: Lobby,
    ) -> Result<()> {
        let (to_cli, from_server) = mpsc::channel(10);
        let tcp_sock_addr = socket.peer_addr().expect("Couldn't get tcp peer address");

        let l_set = lobby.settings.read().await;
        let max_players = l_set.server.max_players;
        let start_udp_handshake = l_set.udp.initiate_handshake;
        drop(l_set);

        let mut conn = Connection::new(socket);

        tracing::debug!("Waiting for client init");
        let connect = conn.read_packet().await?;

        let new_player = match connect.data {
            PacketData::Connect {
                client_name: ref name,
                ref c_type,
                ..
            } => {
                let settings = lobby.settings.read().await;
                if settings.ban_list.players.contains(&connect.id) {
                    let identifier = format!("{} ({}/{})", tcp_sock_addr.to_string(), name, connect.id);
                    tracing::warn!("Banned profile tried to connect: {}", identifier);
                    tracing::info!("Ignoring player {}", identifier);
                    Self::ignore_client(conn, identifier).await?;
                    return Err(SMOError::ClientInit(ClientInitError::BannedID));
                }
                drop(settings);

                // send server init
                tracing::debug!("Send server init");
                conn.write_packet(&Packet::new(
                    Guid::default(),
                    PacketData::Init { max_players },
                ))
                .await?;

                match c_type {
                    ConnectionType::FirstConnection => {
                        let names = lobby.names.0.read().await;
                        let entry_exists =
                            names.contains_left(&connect.id) || names.contains_right(name);
                        if entry_exists {
                            return Err(SMOError::ClientInit(ClientInitError::DuplicateClient));
                        }
                    }
                    ConnectionType::Reconnecting => {}
                }

                // TODO: in case of a reconnect, we need to partially keep the
                // old player data and not create a completely new object.
                // Because older versions of the mod (below 1.3.0) did not send
                // all important packets again after a reconnect.
                let data = PlayerData {
                    name: name.clone(),
                    ipv4: Some(conn.addr.ip()),
                    ..PlayerData::new(to_cli.clone())
                };

                let local_udp_addr =
                    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), udp_port);
                let udp = UdpSocket::bind(local_udp_addr).await?;
                let local_udp_addr = udp.local_addr().expect("Failed to unwrap udp port");
                tracing::debug!("Binding udp to: {:?}", local_udp_addr);

                tracing::debug!("setting new udp connection");
                let udp_conn = UdpConnection::new(udp, tcp_sock_addr.ip());

                if start_udp_handshake {
                    tracing::debug!("Starting udp handshake");
                    conn.write_packet(&Packet::new(
                        Guid::default(),
                        PacketData::UdpInit {
                            port: local_udp_addr.port(),
                        },
                    ))
                    .await?;
                }

                let recv_broadcast = broadcast.subscribe();

                let to_coord = to_coord.clone();
                tracing::debug!("Created client data");
                let client = Client {
                    display_name: name.trim_matches(char::from(0)).to_string(),
                    guid: connect.id,
                    alive: true,
                    to_coord,
                    from_server,
                    conn,
                    udp_conn,
                    send_broadcast: broadcast,
                    recv_broadcast,
                    lobby,
                };

                tracing::debug!("Initialized player");

                Ok(Some(Command::Server(ServerCommand::NewPlayer {
                    cli: client,
                    data,
                    connect_packet: Box::new(connect),
                    comm: to_cli,
                })))
            }
            PacketData::JsonApi { json } => {
                JsonApi::handle(LobbyView::new(&lobby), conn.socket, conn.addr, json, false).await?;
                Ok(None)
            }
            _ => Err(SMOError::ClientInit(ClientInitError::BadHandshake)),
        }?;

        if let Some(player) = new_player {
            to_coord.send(player).await?;
        }
        Ok(())
    }

    pub async fn ignore_client(mut conn: Connection, mut identifier: String) -> Result<()> {
        // send server init (required to crash ignored players later)
        conn.write_packet(&Packet::new(
            Guid::default(),
            PacketData::Init { max_players: 1 },
        )).await?;
        loop {
            match conn.read_packet().await {
                // disconnect
                Err(_) => { break; },
                // client init
                Ok(Packet { id, data: PacketData::Connect { client_name, .. }, .. }) => {
                    identifier = format!("{} ({}/{})", conn.addr.to_string(), client_name, id);
                    tracing::debug!("{} packet received from {}.", "connect", identifier);
                    tracing::info!("Ignoring player {}", identifier);
                },
                // client entered a stage
                Ok(Packet { data: PacketData::Game { stage, .. }, .. }) => {
                    tracing::debug!("{} packet received from {}.", "game", identifier);
                    tracing::info!("Crashing ignored player {} after entering stage {}", identifier, stage);
                    // wait 500ms
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    // crash player
                    conn.write_packet(&Packet::new(
                        Guid::default(),
                        PacketData::ChangeStage {
                            id           : "$among$us/SubArea".to_string(),
                            stage        : "$agogusStage".to_string(),
                            scenario     : 21,
                            sub_scenario : 69,
                        },
                    )).await?;
                },
                // ignore all other packages
                Ok(Packet { data, .. }) => {
                    tracing::debug!("{} packet received from {}.", data.get_type_name(), identifier);
                },
            };
        };
        tracing::info!("Ignored player disconnected {}", identifier);
        Ok(())
    }

    fn get_player(&self) -> Ref<'_, Guid, PlayerData, RandomState> {
        self.lobby
            .players
            .get(&self.guid)
            .expect("Client couldnt find its player data")
    }

    fn get_player_mut(&self) -> RefMut<'_, Guid, PlayerData, RandomState> {
        self.lobby
            .players
            .get_mut(&self.guid)
            .expect("Client couldnt find its player data")
    }
}
