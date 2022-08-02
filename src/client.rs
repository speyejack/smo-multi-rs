use crate::cmds::Command;
use crate::guid::Guid;
use crate::net::PacketData;
use crate::net::{encoding::Decodable, Packet};
use crate::settings::SyncSettings;
use crate::types::Costume;
use anyhow::Result;
use bytes::{Buf, BytesMut};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
    pub socket: TcpStream,
    pub to_coord: mpsc::Sender<Command>,
    pub from_server: mpsc::Receiver<Command>,
    pub buff: bytes::BytesMut,
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

        self.disconnect().await;
        Ok(())
    }

    async fn read_event(&mut self) -> Result<ClientEvent> {
        let event = select! {
            amount = self.socket.read(&mut self.buff[..]) => {
                ClientEvent::Packet(self.parse_packet(amount?)?)
            },
            command = self.from_server.recv() => ClientEvent::Command(command.unwrap()),
        };
        Ok(event)
    }

    pub async fn disconnect(mut self) -> Result<()> {
        self.socket.shutdown();
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
        let read_amount = self.socket.read(&mut self.buff[..]).await?;
        self.parse_packet(read_amount)
    }

    fn parse_packet(&mut self, read_amount: usize) -> Result<Packet> {
        if read_amount == 0 {
            self.alive = false;
        }
        let result: Packet = Packet::decode(&mut self.buff.split())?;
        self.buff.advance(read_amount);
        Ok(result)
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
        let mut buff = BytesMut::with_capacity(300);
        // let mut buffer: [u8; 100] = [0; 100];
        // let mut cursor = Cursor::new(&mut buffer[..]);
        // let size = bincode::serialized_size(&packet).unwrap() as usize;
        // bincode::serialize_into(&mut cursor, &packet)?;
        // let amount = self.socket.write(&buffer[..size]).await?;
        // assert!(amount == size);
        Ok(())
    }

    async fn initialize_client(
        socket: TcpStream,
        to_coord: mpsc::Sender<Command>,
        settings: SyncSettings,
    ) -> Result<(Self, mpsc::Sender<Command>)> {
        let (to_cli, from_server) = mpsc::channel(10);

        let l_set = settings.read().await;
        let max_players = l_set.max_players;
        drop(l_set);

        let data = ClientData {
            settings,
            ..ClientData::default()
        };
        let data = Arc::new(RwLock::new(data));

        let mut client = Client {
            data,
            guid: Guid::default(),
            alive: true,
            socket,
            to_coord,
            from_server,
            buff: BytesMut::with_capacity(300),
        };

        client
            .send_packet(&Packet::new(client.guid, PacketData::Init { max_players }))
            .await?;
        let connect = client.recv_packet().await?;
        // TODO Verified max connected players
        client.guid = connect.id;
        match connect.data {
            PacketData::Connect {
                c_type,
                max_player,
                client_name,
            } => {
                // TODO Check if connection is new or reconnecting
                // Then figure out if any stale clients remaining and remove them.
            }
            _ => return Err(anyhow::anyhow!("Failed to initialize client")),
        }

        todo!()
    }
}
