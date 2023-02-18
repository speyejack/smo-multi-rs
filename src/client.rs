use crate::{
    cmds::{ClientCommand, Command, ServerCommand},
    guid::Guid,
    net::{connection::Connection, udp_conn::UdpConnection, Packet, PacketData},
    settings::SyncSettings,
    types::{ChannelError, ClientInitError, Costume, ErrorSeverity, Result, SMOError, Vector3},
};
use enet::peer::Peer;
use nalgebra::UnitQuaternion;
use std::{
    collections::HashSet,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::AsyncWriteExt,
    net::{TcpStream, UdpSocket},
    select,
    sync::{broadcast, mpsc, RwLock},
};
use tracing::Level;

pub type SyncPlayer = Arc<RwLock<PlayerData>>;

#[derive(Debug)]
pub struct Client {
    pub display_name: String,
    pub player: SyncPlayer,
    pub guid: Guid,
    pub alive: bool,
    pub conn: Connection,
    pub to_coord: mpsc::Sender<Command>,
    pub from_server: mpsc::Receiver<ClientCommand>,
    pub send_broadcast: broadcast::Sender<ClientCommand>,
    pub recv_broadcast: broadcast::Receiver<ClientCommand>,
    pub settings: SyncSettings,
}

#[derive(Clone, Debug, Default)]
pub struct PlayerData {
    pub ipv4: Option<IpAddr>,
    pub name: String,
    pub shine_sync: HashSet<i32>,
    pub scenario: i8,
    pub is_2d: bool,
    pub is_seeking: bool,
    pub last_game_packet: Option<Packet>,
    pub speedrun_start: bool,
    pub loaded_save: bool,
    pub time: Duration,
    pub costume: Option<Costume>,
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
        self.conn.peer.disconnect().await;
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
                let settings = self.settings.read().await;
                if settings.flip.enabled
                    && settings.flip.pov.is_others_flip()
                    && settings.flip.players.get(&packet.id).is_some()
                {
                    let angle = std::f32::consts::PI;
                    let rot_quad = *(UnitQuaternion::from_axis_angle(&Vector3::z_axis(), angle));
                    let data = self.player.read().await;
                    *pos += get_mario_size(data.is_2d) * Vector3::y();
                    *rot *= rot_quad;
                }
                PacketDestination::Coordinator
            }
            PacketData::Costume(costume) => {
                let mut data = self.player.write().await;
                data.costume = Some(costume.clone());
                data.loaded_save = true;
                PacketDestination::Coordinator
            }
            PacketData::Game {
                is_2d,
                scenario_num,
                stage,
            } => {
                let mut data = self.player.write().await;
                data.is_2d = *is_2d;
                data.scenario = *scenario_num;
                if stage == "CapWorldHomeStage" && *scenario_num == 0 {
                    data.speedrun_start = true;
                    data.shine_sync.clear();
                }
                let new_packet = packet.clone();
                data.last_game_packet = Some(new_packet);
                PacketDestination::Coordinator
            }
            PacketData::Tag {
                update_type,
                is_it,
                seconds,
                minutes,
            } => {
                let mut data = self.player.write().await;
                match update_type {
                    crate::net::TagUpdate::Time => {
                        data.time = Duration::from_secs(*seconds as u64 + *minutes as u64 * 60);
                    }
                    crate::net::TagUpdate::State => {
                        data.is_seeking = *is_it;
                    }
                }
                PacketDestination::Broadcast
            }
            PacketData::Shine { shine_id, .. } => {
                let mut data = self.player.write().await;
                if data.loaded_save {
                    data.shine_sync.insert(*shine_id);
                }
                PacketDestination::Coordinator
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
                        let settings = self.settings.read().await;
                        if settings.flip.enabled
                            && settings.flip.pov.is_self_flip()
                            && settings.flip.players.get(&self.guid).is_some()
                            && settings.flip.players.get(&p.id).is_none()
                        {
                            let angle = std::f32::consts::PI;
                            let rot_quad =
                                *(UnitQuaternion::from_axis_angle(&Vector3::z_axis(), angle));
                            let data = self.player.read().await;
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
                    PacketData::Shine { shine_id, .. } => {
                        let mut data = self.player.write().await;
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

        self.conn.write_packet(packet).await
    }

    /// Readdress packet to come from the same guid as client then send
    pub async fn readdress_and_send(&mut self, p: &mut Packet) -> Result<()> {
        p.id = self.guid;
        self.send_packet(p).await
    }

    /// Perform the initialization and handshake with client then hand off to coordinator
    pub async fn initialize_client(
        peer: Peer,
        to_coord: mpsc::Sender<Command>,
        broadcast: broadcast::Sender<ClientCommand>,
        settings: SyncSettings,
    ) -> Result<()> {
        let (to_cli, from_server) = mpsc::channel(10);

        let l_set = settings.read().await;
        let max_players = l_set.server.max_players;
        let start_udp_handshake = l_set.udp.initiate_handshake;
        drop(l_set);

        tracing::debug!("Initializing connection");
        let mut conn = Connection::new(peer);
        conn.write_packet(&Packet::new(
            Guid::default(),
            PacketData::Init { max_players },
        ))
        .await?;

        tracing::debug!("Waiting for reply");
        let connect = conn.read_packet().await?;

        let new_player = match connect.data {
            PacketData::Connect {
                client_name: ref name,
                ..
            } => {
                let data = PlayerData {
                    name: name.clone(),
                    ipv4: Some(conn.addr.ip()),
                    ..PlayerData::default()
                };

                if start_udp_handshake {
                    tracing::debug!("Starting udp handshake");
                }

                let data = Arc::new(RwLock::new(data));
                let recv_broadcast = broadcast.subscribe();

                let to_coord = to_coord.clone();
                tracing::debug!("Created client data");
                let client = Client {
                    display_name: name.trim_matches(char::from(0)).to_string(),
                    player: data,
                    guid: connect.id,
                    alive: true,
                    settings,
                    to_coord,
                    from_server,
                    conn,
                    send_broadcast: broadcast,
                    recv_broadcast,
                };

                tracing::debug!("Initialized player");

                Ok(Command::Server(ServerCommand::NewPlayer {
                    cli: client,
                    connect_packet: Box::new(connect),
                    comm: to_cli,
                }))
            }
            _ => Err(SMOError::ClientInit(ClientInitError::BadHandshake)),
        }?;

        to_coord.send(new_player).await?;
        Ok(())
    }
}
