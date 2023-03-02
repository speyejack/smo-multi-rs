use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use serde::Deserialize;
use serde_json::{from_str, json, Value};
use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt, BufWriter};
use tokio::net::{TcpListener, TcpStream};

use crate::coordinator::Coordinator;
use crate::json_api::{BlockClients, JsonApiCommands, JsonApiStatus};
use crate::lobby::LobbyView;
use crate::net::connection::Connection;
use crate::types::Result;

pub(crate) struct JsonApi {
    listener: TcpListener,
    view: LobbyView,
}

impl JsonApi {
    pub async fn create(view: LobbyView) -> Result<Option<Self>> {
        let settings = view.get_lobby().settings.read().await;
        if !settings.json_api.enabled {
            return Ok(None);
        }
        // TcpListener.bind.json_api.port
        let listener = TcpListener::bind(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
            settings.json_api.port,
        ))
        .await?;
        drop(settings);

        tracing::trace!("Created json api");
        Ok(Some(Self { listener, view }))
    }

    pub async fn loop_events(mut self) -> Result<()> {
        tracing::trace!("Starting json loop");
        loop {
            let (stream, ip): (TcpStream, SocketAddr) = tokio::select! {
                conn = self.listener.accept() => {
                    conn?
                },
                _ = self.view.get_server_recv().recv() => {
                    return Ok(())
                }
            };

            tracing::trace!("Got json event");
            let mut stream = BufWriter::new(stream);
            let mut buff = [0; 1000];
            let read_count = stream.read(&mut buff).await;
            if read_count.is_err() {
                continue;
            }

            let json_str = String::from_utf8(buff[..read_count.unwrap()].to_vec());
            if let Ok(json_str) = json_str {
                let result = self.handle(stream, ip, json_str).await;
                if let Err(e) = result {
                    tracing::error!("Json api: {}", e);
                }
            }
        }
    }

    pub async fn handle(
        &mut self,
        mut socket: BufWriter<TcpStream>,
        addr: SocketAddr,
        json_str: String,
    ) -> Result<()> {
        let settings = self.view.get_lobby().settings.read().await;

        if !settings.json_api.enabled {
            return Ok(());
        }

        if BlockClients::is_blocked(&addr).await {
            tracing::info!("Rejected blocked client {}", addr.ip());
            return Ok(());
        }

        tracing::debug!("request: {}", json_str);
        let packet: JsonApiPacket = match from_str(&json_str) {
            Ok(p) => p,
            Err(_) => {
                tracing::warn!("Invalid request from {}", addr.ip());
                BlockClients::fail(&addr).await;
                return Ok(());
            }
        };

        let req: JsonApiRequest = packet.request;

        if !["Status", "Command", "Permissions"].contains(&&*req.kind) {
            tracing::warn!("Invalid Type from {}", addr.ip());
            BlockClients::fail(&addr).await;
            return Ok(());
        }

        if !settings.json_api.tokens.contains_key(&req.token) {
            tracing::warn!("Invalid Token from {}", addr.ip());
            BlockClients::fail(&addr).await;
            return Ok(());
        }

        let response: Value = match req.kind.as_str() {
            "Status" => json!(JsonApiStatus::create(&self.view, &req.token).await),
            "Permissions" => json!({
                "Permissions": settings.json_api.tokens[&req.token],
            }),
            "Command" => {
                drop(settings);
                json!(JsonApiCommands::process(&mut self.view, &req.token, &req.data).await)
            }
            _ => json!({
                "Error": ([req.kind, " is not implemented yet".to_string()].join("")),
            }),
        };

        BlockClients::redeem(&addr).await;
        JsonApi::respond(&mut socket, response.to_string()).await
    }

    async fn respond(mut socket: &mut BufWriter<TcpStream>, response_str: String) -> Result<()> {
        // TODO Repeat write until all bytes are sent
        let _ = socket.write(&response_str.as_bytes()).await?;
        socket.flush().await?;
        tracing::trace!("response: {}", response_str);
        Ok(())
    }
}

#[derive(Deserialize)]
struct JsonApiRequest {
    #[serde(rename = "Type")]
    kind: String,

    #[serde(rename = "Token")]
    token: String,

    #[serde(rename = "Data")]
    data: Option<String>,
}

#[derive(Deserialize)]
struct JsonApiPacket {
    #[serde(rename = "API_JSON_REQUEST")]
    request: JsonApiRequest,
}
