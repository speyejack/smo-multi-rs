use crate::cmds::BroadcastCommand;
use crate::cmds::Command;
use crate::cmds::ServerCommand;
use crate::guid::Guid;

use crate::net::connection::Connection;
use crate::net::udp_conn::UdpConnection;
use crate::net::Packet;
use crate::net::PacketData;
use crate::settings::SyncSettings;
use crate::types::ClientInitError;
use crate::types::ErrorSeverity;
use crate::types::{Costume, SMOError};
use crate::types::{EncodingError, Result};
use std::collections::HashSet;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::net::UdpSocket;
use tokio::select;
use tokio::sync::broadcast;
use tokio::sync::{mpsc, RwLock};

pub type SyncPlayer = Arc<RwLock<PlayerData>>;

#[derive(Debug)]
pub struct Client {
    pub display_name: String,
    pub player: SyncPlayer,
    pub guid: Guid,
    pub alive: bool,
    pub conn: Connection,
    pub udp_conn: UdpConnection,
    pub to_coord: mpsc::Sender<Command>,
    pub from_server: mpsc::Receiver<Command>,
    pub from_others: broadcast::Receiver<BroadcastCommand>,
    pub to_others: broadcast::Sender<BroadcastCommand>,
}

#[derive(Clone, Debug, Default)]
pub struct PlayerData {
    pub name: String,
    pub shine_sync: HashSet<i32>,
    pub scenario: i8,
    pub is_2d: bool,
    pub is_seeking: bool,
    pub last_game_packet: Option<Packet>,
    pub speedrun_start: bool,
    pub loaded_save: bool,
    pub time: Duration,
    pub settings: SyncSettings,
    pub costume: Costume,
}

#[derive(Debug)]
enum Origin {
    Internal,
    External,
}

#[derive(Debug)]
enum ClientEvent {
    Packet(Packet),
    Command(Command),
}

enum SendLocation {
    Broadcast,
    Coordinator,
    NoSend,
}

impl Client {
    pub async fn handle_events(mut self) -> Result<()> {
        while self.alive {
            let event = self.read_event().await;

            tracing::trace!("Event: {:?}", &event);
            let result = match event {
                Ok((Origin::External, ClientEvent::Packet(p))) => self.handle_packet(p).await,
                Ok((Origin::Internal, ClientEvent::Packet(p))) => self.send_packet(&p).await,
                Ok((_, ClientEvent::Command(c))) => self.handle_command(c).await,
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

    async fn read_event(&mut self) -> Result<(Origin, ClientEvent)> {
        let event = select! {
            packet = self.conn.read_packet() => {
                (Origin::External, ClientEvent::Packet(packet?))
            },
            udp_packet = self.udp_conn.read_packet() => {
                tracing::trace!("Got udp event!");
                (Origin::External, ClientEvent::Packet(udp_packet?))
            },
            command = self.from_server.recv() => (Origin::Internal, ClientEvent::Command(command.ok_or(SMOError::RecvChannel)?)),
            packet = self.from_others.recv() => (Origin::Internal, ClientEvent::Packet(match (packet)? {
                BroadcastCommand::Packet(e) => e,
            })),
        };
        Ok(event)
    }

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

    async fn handle_packet(&mut self, packet: Packet) -> Result<()> {
        tracing::debug!("Handling packet: {}", &packet.data.get_type_name());
        let send_location = match &packet.data {
            PacketData::Costume(costume) => {
                // TODO: Figure out why shine sync code in original
                // code base for costume packet
                let mut data = self.player.write().await;
                data.costume = costume.clone();
                data.loaded_save = true;
                SendLocation::Coordinator
            }
            PacketData::Game {
                is_2d,
                scenario_num,
                stage,
            } => {
                let mut data = self.player.write().await;
                data.is_2d = *is_2d;
                data.scenario = *scenario_num;
                data.last_game_packet = Some(packet.clone());
                if stage == "CapWorldHomeStage" && *scenario_num == 0 {
                    data.speedrun_start = true;
                    data.shine_sync.clear();
                }

                SendLocation::Coordinator
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
                SendLocation::Broadcast
            }
            PacketData::Shine { shine_id, .. } => {
                let mut data = self.player.write().await;
                if data.loaded_save {
                    data.shine_sync.insert(*shine_id);
                }
                SendLocation::Coordinator
            }
            PacketData::UdpInit { port } => {
                self.udp_conn.set_client_port(*port);
                SendLocation::NoSend
            }
            _ => SendLocation::Broadcast,
        };

        match send_location {
            SendLocation::Broadcast => {
                self.to_others.send(BroadcastCommand::Packet(packet))?;
            }
            SendLocation::Coordinator => self.to_coord.send(Command::Packet(packet)).await?,
            SendLocation::NoSend => {}
        }

        Ok(())
    }

    pub async fn recv_packet(&mut self) -> Result<Packet> {
        self.conn.read_packet().await
    }

    fn parse_packet(&mut self) -> Result<Packet> {
        match self.conn.parse_packet() {
            Err(e) => Err(e),
            Ok(Some(t)) => Ok(t),
            Ok(None) => Err(EncodingError::NotEnoughData.into()),
        }
    }

    async fn handle_command(&mut self, command: Command) -> Result<()> {
        match command {
            Command::Packet(p) => {
                if p.id == self.guid {
                    if let crate::net::PacketData::Disconnect = p.data {
                        self.alive = false;
                    }
                } else {
                    self.send_packet(&p).await?;
                }
            }
            _ => todo!(),
        }
        Ok(())
    }

    pub async fn send_packet(&mut self, packet: &Packet) -> Result<()> {
        // TODO Handle disconnect packets
        if packet.id != self.guid {
            tracing::debug!(
                "Sending packet: {}->{}",
                packet.id,
                packet.data.get_type_name()
            );
            tracing::debug!("Udp conn: {:?}", self.udp_conn);

            if self.udp_conn.is_client_udp() {
                // Use UDP traffic
                match packet.data {
                    PacketData::Player { .. } | PacketData::Cap { .. } => {
                        self.udp_conn.write_packet(packet).await
                    }
                    _ => self.conn.write_packet(packet).await,
                }
            } else {
                // Fall back to TCP traffic
                self.conn.write_packet(packet).await
            }
        } else {
            Ok(())
        }
    }

    pub async fn initialize_client(
        socket: TcpStream,
        to_coord: mpsc::Sender<Command>,
        to_others: broadcast::Sender<BroadcastCommand>,
        from_others: broadcast::Receiver<BroadcastCommand>,
        udp_port: u16,
        settings: SyncSettings,
    ) -> Result<()> {
        let (to_cli, from_server) = mpsc::channel(10);
        let tcp_sock_addr = socket.peer_addr().expect("Couldn't get tcp peer address");

        let l_set = settings.read().await;
        let max_players = l_set.server.max_players;
        drop(l_set);

        tracing::debug!("Initializing connection");
        let mut conn = Connection::new(socket);
        conn.write_packet(&Packet::new(
            Guid::default(),
            PacketData::Init { max_players },
        ))
        .await?;

        let local_udp_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), udp_port);
        let udp = UdpSocket::bind(local_udp_addr).await?;
        let local_udp_addr = udp.local_addr().expect("Failed to unwrap udp port");
        tracing::debug!("Binding udp to: {:?}", local_udp_addr);

        tracing::debug!("setting new udp connection");
        let udp_conn = UdpConnection::new(udp, tcp_sock_addr.ip());

        tracing::debug!("Waiting for reply");
        let connect = conn.read_packet().await?;

        let new_player = match connect.data {
            PacketData::Connect {
                client_name: ref name,
                ..
            } => {
                let data = PlayerData {
                    settings,
                    name: name.clone(),
                    ..PlayerData::default()
                };

                conn.write_packet(&Packet::new(
                    Guid::default(),
                    PacketData::UdpInit {
                        port: local_udp_addr.port(),
                    },
                ))
                .await?;

                let data = Arc::new(RwLock::new(data));

                let to_coord = to_coord.clone();
                tracing::debug!("Created client data");
                let client = Client {
                    display_name: name.trim_matches(char::from(0)).to_string(),
                    player: data,
                    guid: connect.id,
                    alive: true,
                    to_coord,
                    from_server,
                    conn,
                    udp_conn,
                    to_others,
                    from_others,
                };

                Ok(Command::Server(ServerCommand::NewPlayer {
                    cli: client,
                    connect_packet: Box::new(connect),
                    comm: to_cli,
                }))
            }
            _ => Err(SMOError::ClientInit(ClientInitError::BadHandshake)),
        }?;
        tracing::debug!("Initialized player");

        to_coord.send(new_player).await?;
        Ok(())
    }
}
