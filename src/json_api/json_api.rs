use serde::Deserialize;
use serde_json::{from_str, json, Value};
use tokio::io::AsyncWriteExt;

use crate::coordinator::Coordinator;
use crate::json_api::{BlockClients, JsonApiCommands, JsonApiStatus};
use crate::lobby::LobbyView;
use crate::net::connection::Connection;
use crate::types::Result;

pub(crate) struct JsonApi {}

impl JsonApi {
    pub async fn handle(view: &LobbyView, conn: Connection, json_str: String) -> Result<()> {
        let settings = view.get_lobby().settings.read().await;

        if !settings.json_api.enabled {
            return Ok(());
        }

        if BlockClients::is_blocked(&conn).await {
            tracing::info!("Rejected blocked client {}", conn.addr.ip());
            return Ok(());
        }

        tracing::debug!("request: {}", json_str);
        let packet: JsonApiPacket = match from_str(&json_str) {
            Ok(p) => p,
            Err(_) => {
                tracing::warn!("Invalid request from {}", conn.addr.ip());
                BlockClients::fail(&conn).await;
                return Ok(());
            }
        };

        let req: JsonApiRequest = packet.request;

        if !["Status", "Command", "Permissions"].contains(&&*req.kind) {
            tracing::warn!("Invalid Type from {}", conn.addr.ip());
            BlockClients::fail(&conn).await;
            return Ok(());
        }

        if !settings.json_api.tokens.contains_key(&req.token) {
            tracing::warn!("Invalid Token from {}", conn.addr.ip());
            BlockClients::fail(&conn).await;
            return Ok(());
        }

        let response: Value = match req.kind.as_str() {
            "Status" => json!(JsonApiStatus::create(view, &req.token).await),
            "Permissions" => json!({
                "Permissions": settings.json_api.tokens[&req.token],
            }),
            "Command" => {
                drop(settings);
                json!(JsonApiCommands::process(view, &req.token, &req.data).await)
            }
            _ => json!({
                "Error": ([req.kind, " is not implemented yet".to_string()].join("")),
            }),
        };

        BlockClients::redeem(&conn).await;
        JsonApi::respond(conn, response.to_string()).await
    }

    async fn respond(mut conn: Connection, response_str: String) -> Result<()> {
        conn.socket.write(&response_str.as_bytes()).await?;
        conn.socket.flush().await?;
        tracing::debug!("response: {}", response_str);
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
