use crate::cmds::Command;
use crate::cmds::ServerCommand;
use crate::guid::Guid;
use crate::net::connection;
use crate::net::connection::Connection;
use crate::net::udp_conn::UdpConnection;
use crate::net::Packet;
use crate::net::PacketData;
use crate::settings::SyncSettings;
use crate::types::ClientInitError;
use crate::types::{Costume, SMOError};
use crate::types::{EncodingError, Result};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::net::UdpSocket;
use tokio::select;
use tokio::sync::{mpsc, RwLock};
use tracing::info;
use tracing::info_span;
use tracing::instrument;

pub type ClientMap = HashMap<Guid, SyncClient>;
pub type SyncClient = Arc<RwLock<ClientData>>;

#[derive(Debug)]
pub struct Client {
    pub display_name: String,
    pub data: SyncClient,
    pub guid: Guid,
    pub alive: bool,
    pub conn: Connection,
    pub udp_conn: UdpConnection,
    pub to_coord: mpsc::Sender<Command>,
    pub from_server: mpsc::Receiver<Command>,
}

#[derive(Default, Clone, Debug)]
pub struct ClientData {
    pub name: String,
    pub shine_sync: HashSet<i32>,
    pub scenario: u8,
    pub is_2d: bool,
    pub is_seeking: bool,
    pub last_game_packet: Option<Packet>,
    pub speedrun: bool,
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

impl Client {
    pub async fn handle_events(mut self) -> Result<()> {
        while self.alive {
            let event = self.read_event().await;

            tracing::trace!("Event: {:?}", &event);
            let result = match event {
                Ok((Origin::External, ClientEvent::Packet(p))) => self.handle_packet(p).await,
                Ok((Origin::Internal, ClientEvent::Packet(p))) => self.send_packet(&p).await,
                Ok((_, ClientEvent::Command(c))) => self.handle_command(c).await,
                Err(SMOError::Encoding(EncodingError::ConnectionClose))
                | Err(SMOError::Encoding(EncodingError::ConnectionReset))
                | Err(SMOError::RecvChannel) => {
                    self.alive = false;
                    break;
                }
                Err(e) => Err(e),
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
                tracing::debug!("Got udp event!");
                (Origin::External, ClientEvent::Packet(udp_packet?))
            },
            command = self.from_server.recv() => (Origin::Internal, ClientEvent::Command(command.ok_or(SMOError::RecvChannel)?)),
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
        let send_to_coord = match &packet.data {
            PacketData::Costume(costume) => {
                // TODO: Figure out why shine sync code in original
                // code base for costume packet
                let mut data = self.data.write().await;
                data.costume = costume.clone();
                data.loaded_save = true;
                true
            }
            PacketData::Game {
                is_2d,
                scenario_num,
                stage,
            } => {
                let mut data = self.data.write().await;
                data.is_2d = *is_2d;
                data.scenario = *scenario_num;
                data.last_game_packet = Some(packet.clone());
                if stage == "CapWorldHomeStage" && *scenario_num == 0 {
                    data.speedrun = true;
                    data.shine_sync.clear();
                }

                true
            }
            PacketData::Tag {
                update_type,
                is_it,
                seconds,
                minutes,
            } => {
                let mut data = self.data.write().await;
                match update_type {
                    crate::net::TagUpdate::Time => {
                        data.time = Duration::from_secs(*seconds as u64 + *minutes as u64 * 60);
                    }
                    crate::net::TagUpdate::State => {
                        data.is_seeking = *is_it;
                    }
                }
                true
            }
            PacketData::Shine { shine_id, .. } => {
                let mut data = self.data.write().await;
                if data.loaded_save {
                    data.shine_sync.insert(*shine_id);
                }
                true
            }
            PacketData::UdpInit { port } => {
                let ip = self.udp_conn.send_addr.ip();
                self.udp_conn.send_addr = SocketAddr::new(ip, *port);
                false
            }
            _ => true,
        };

        if send_to_coord {
            self.to_coord.send(Command::Packet(packet)).await?;
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

            match packet.data {
                PacketData::Player { .. } => self.udp_conn.write_packet(packet).await,
                _ => self.conn.write_packet(packet).await,
            }
        } else {
            Ok(())
        }
    }

    pub async fn initialize_client(
        socket: TcpStream,
        to_coord: mpsc::Sender<Command>,
        settings: SyncSettings,
    ) -> Result<()> {
        let (to_cli, from_server) = mpsc::channel(10);
        let tcp_sock_addr = socket.peer_addr().expect("Couldn't get tcp peer address");

        let l_set = settings.read().await;
        let max_players = l_set.max_players;
        drop(l_set);

        tracing::debug!("Initializing connection");
        let mut conn = Connection::new(socket);
        conn.write_packet(&Packet::new(
            Guid::default(),
            PacketData::Init { max_players },
        ))
        .await?;

        let udp = UdpSocket::bind("0.0.0.0:0").await?;
        let udp_port = udp.local_addr().expect("Failed to unwrap udp port").port();
        conn.write_packet(&Packet::new(
            Guid::default(),
            PacketData::UdpInit { port: udp_port },
        ))
        .await?;

        tracing::debug!("setting new udp connection");
        let mut udp_conn = UdpConnection::new(udp, SocketAddr::new(tcp_sock_addr.ip(), 55446));

        tracing::debug!("Waiting for reply");
        let new_player = loop {
            let connect = conn.read_packet().await?;

            let new_player = match connect.data {
                PacketData::UdpInit { port } => {
                    udp_conn = UdpConnection::new(
                        udp_conn.socket,
                        SocketAddr::new(tcp_sock_addr.ip(), port),
                    );
                }
                PacketData::Connect {
                    client_name: ref name,
                    ..
                } => {
                    let data = ClientData {
                        settings,
                        name: name.clone(),
                        ..ClientData::default()
                    };

                    let data = Arc::new(RwLock::new(data));

                    let to_coord = to_coord.clone();
                    tracing::debug!("Created client data");
                    let client = Client {
                        display_name: name.trim_matches(char::from(0)).to_string(),
                        data,
                        guid: connect.id,
                        alive: true,
                        to_coord,
                        from_server,
                        conn,
                        udp_conn,
                    };

                    break Ok(Command::Server(ServerCommand::NewPlayer {
                        cli: client,
                        connect_packet: Box::new(connect),
                        comm: to_cli,
                    }));
                }
                _ => break Err(SMOError::ClientInit(ClientInitError::BadHandshake)),
            };
        }?;

        to_coord.send(new_player).await?;
        Ok(())
    }
}
