use serde::Deserialize;
use serde_json::{ from_str, json, Value };
use std::collections::HashMap;
use tokio::io::AsyncWriteExt;


use crate::guid::Guid;
use crate::json_api::{ JsonApiStatus, BlockClients };
use crate::net::connection::Connection;
use crate::player_holder::PlayerInfo;
use crate::settings::SyncSettings;
use crate::types::Result;


pub(crate) struct JsonApi {}


impl JsonApi {
    pub async fn handle(
        sync_settings: &SyncSettings,
        clients: &HashMap<Guid, PlayerInfo>,
        conn: Connection,
        json_str: String
    ) -> Result<()> {
        let settings = sync_settings.read().await;

        if !settings.json_api.enabled { return Ok(()); }

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

        if ![ "Status", "Command", "Permissions" ].contains(&&*req.kind) {
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
            "Status" => json!(JsonApiStatus::create(sync_settings, &req.token, clients).await),
            "Permissions" => json!({
                "Permissions": settings.json_api.tokens[&req.token],
            }),
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
    kind : String,

    #[serde(rename = "Token")]
    token : String,

    // @todo: implement when CLI commands get implemented
    //#[serde(rename = "Data")]
    //data : Option<String>,
}


#[derive(Deserialize)]
struct JsonApiPacket {
    #[serde(rename = "API_JSON_REQUEST")]
    request: JsonApiRequest,
}
