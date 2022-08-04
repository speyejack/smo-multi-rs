use crate::cmds::Command;
use crate::guid::Guid;
use crate::net::connection::Connection;
use crate::net::Packet;
use crate::net::PacketData;
use crate::settings::SyncSettings;
use crate::types::{Costume, SMOError};
use crate::types::{EncodingError, Result};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::select;
use tokio::sync::{mpsc, RwLock};

pub type ClientMap = HashMap<Guid, SyncClient>;
pub type SyncClient = Arc<RwLock<ClientData>>;

#[derive(Debug)]
pub struct Client {
    pub data: SyncClient,
    pub guid: Guid,
    pub alive: bool,
    pub conn: Connection,
    pub to_coord: mpsc::Sender<Command>,
    pub from_server: mpsc::Receiver<Command>,
}

#[derive(Default, Clone, Debug)]
pub struct ClientData {
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

enum ClientEvent {
    Packet(Packet),
    Command(Command),
}

impl Client {
    pub async fn handle_events(mut self) -> Result<()> {
        while self.alive {
            let event = self.read_event().await;
            let result = match event {
                Ok(ClientEvent::Packet(p)) => self.handle_packet(p).await,
                Ok(ClientEvent::Command(c)) => self.handle_command(c).await,
                Err(e) => Err(e),
            };

            if let Err(e) = result {
                log::warn!("Error with client {}: {}", self.guid, e)
            }
        }

        self.disconnect().await?;
        Ok(())
    }

    async fn read_event(&mut self) -> Result<ClientEvent> {
        let event = select! {
            packet = self.conn.read_packet() => {
                ClientEvent::Packet(packet?)
            },
            command = self.from_server.recv() => ClientEvent::Command(command.unwrap()),
        };
        Ok(event)
    }

    pub async fn disconnect(mut self) -> Result<()> {
        self.conn.socket.shutdown().await?;
        Ok(())
    }

    async fn handle_packet(&mut self, packet: Packet) -> Result<()> {
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
            PacketData::Shine { shine_id } => {
                let mut data = self.data.write().await;
                if data.loaded_save {
                    data.shine_sync.insert(*shine_id);
                }
                true
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
            _ => unimplemented!(),
        }
        Ok(())
    }

    pub async fn send_packet(&mut self, packet: &Packet) -> Result<()> {
        self.conn.write_packet(packet).await
    }

    pub async fn initialize_client(
        socket: TcpStream,
        to_coord: mpsc::Sender<Command>,
        settings: SyncSettings,
    ) -> Result<(Self, mpsc::Sender<Command>)> {
        log::debug!("Starting client initializaiton");
        let (_to_cli, from_server) = mpsc::channel(10);

        let l_set = settings.read().await;
        let max_players = l_set.max_players;
        drop(l_set);

        log::debug!("Verified max players");
        let data = ClientData {
            settings,
            ..ClientData::default()
        };
        let data = Arc::new(RwLock::new(data));

        let mut conn = Connection::new(socket);

        log::debug!("Created client");
        conn.write_packet(&Packet::new(
            Guid::default(),
            PacketData::Init { max_players },
        ))
        .await?;
        log::debug!("Sent init packet");
        let connect = conn.read_packet().await?;
        log::debug!("Received connect packet");
        // TODO Verified max connected players
        match connect.data {
            PacketData::Connect {
                c_type: _,
                max_player: _,
                client_name: _,
            } => {
                log::debug!("Created client data");
                let _client = Client {
                    data,
                    guid: Guid::default(),
                    alive: true,
                    to_coord,
                    from_server,
                    conn,
                };
                // TODO Check if connection is new or reconnecting
                // Then figure out if any stale clients remaining and remove them.
                todo!()
            }
            _ => Err(SMOError::ClientInit),
        }

        // TODO Have coordinator verify that this user can be online.
        // Check max players and whitelist/banlist

        // Have coordinator call setup_client
        // TODO Send all of the last game packets to the client
    }
}
