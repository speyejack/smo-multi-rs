use crate::cmds::ClientCommand;
use crate::cmds::Command;
use crate::cmds::ServerCommand;
use crate::guid::Guid;

use crate::net::connection::Connection;
use crate::net::udp_conn::UdpConnection;
use crate::net::Packet;
use crate::net::PacketData;
use crate::settings::SyncSettings;
use crate::types::ChannelError;
use crate::types::ClientInitError;
use crate::types::ErrorSeverity;
use crate::types::Result;
use crate::types::{Costume, SMOError};
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
use tokio::sync::{broadcast, mpsc, RwLock};

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
    pub from_server: mpsc::Receiver<ClientCommand>,
    pub send_broadcast: broadcast::Sender<ClientCommand>,
    pub recv_broadcast: broadcast::Receiver<ClientCommand>,
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
enum ClientEvent {
    Incoming(Packet),
    Outgoing(ClientCommand),
}

#[derive(Debug)]
enum PacketDestination {
    NoSend,
    Broadcast,
    Coordinator,
}

impl Client {
    pub async fn handle_events(mut self) -> Result<()> {
        while self.alive {
            let event = self.read_event().await;

            tracing::trace!("Event: {:?}", &event);
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

    async fn read_event(&mut self) -> Result<ClientEvent> {
        let event = select! {
            packet = self.conn.read_packet() => {
                ClientEvent::Incoming(packet?)
            },
            udp_packet = self.udp_conn.read_packet() => {
                tracing::trace!("Got udp event!");
                ClientEvent::Incoming(udp_packet?)
            },
            command = self.from_server.recv() => ClientEvent::Outgoing(command.ok_or(ChannelError::RecvChannel)?),
            command = self.recv_broadcast.recv() => ClientEvent::Outgoing(command?),
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
        let send_destination = match &packet.data {
            PacketData::Costume(costume) => {
                // TODO: Figure out why shine sync code in original
                // code base for costume packet
                let mut data = self.player.write().await;
                data.costume = costume.clone();
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
                data.last_game_packet = Some(packet.clone());
                if stage == "CapWorldHomeStage" && *scenario_num == 0 {
                    data.speedrun_start = true;
                    data.shine_sync.clear();
                }

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
            PacketData::UdpInit { port } => {
                self.udp_conn.set_client_port(*port);
                PacketDestination::NoSend
            }
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

    async fn handle_command(&mut self, command: ClientCommand) -> Result<()> {
        match command {
            ClientCommand::Packet(p) => {
                if p.id == self.guid {
                    if let crate::net::PacketData::Disconnect = p.data {
                        self.alive = false;
                    }
                } else {
                    self.send_packet(&p).await?;
                }
            }
            ClientCommand::SelfAddressed(mut p) => self.readdress_and_send(&mut p).await?,
        }
        Ok(())
    }

    pub async fn send_packet(&mut self, packet: &Packet) -> Result<()> {
        // TODO Handle disconnect packets
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
    }

    pub async fn readdress_and_send(&mut self, p: &mut Packet) -> Result<()> {
        p.id = self.guid;
        self.send_packet(p).await
    }

    pub async fn initialize_client(
        socket: TcpStream,
        to_coord: mpsc::Sender<Command>,
        broadcast: broadcast::Sender<ClientCommand>,
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
                let recv_broadcast = broadcast.subscribe();

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
                    send_broadcast: broadcast,
                    recv_broadcast,
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
