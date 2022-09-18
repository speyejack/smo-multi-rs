use serde::Serialize;
use std::collections::HashMap;
use std::net::IpAddr;


use crate::guid::Guid;
use crate::net::{ Packet, PacketData };
use crate::player_holder::PlayerInfo;
use crate::settings::SyncSettings;


#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub(in crate::json_api) struct JsonApiStatusPlayer {
    #[serde(skip_serializing_if = "Option::is_none", rename = "ID")]
    id : Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    name : Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    stage : Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    scenario : Option<i8>,

    #[serde(skip_serializing_if = "Option::is_none")]
    costume : Option<JsonApiStatusPlayerCostume>,

    #[serde(skip_serializing_if = "Option::is_none", rename = "IPv4")]
    ipv4 : Option<IpAddr>,
}


impl JsonApiStatusPlayer {
    pub async fn create(
        sync_settings : &SyncSettings,
        token         : &String,
        clients       : &HashMap<Guid, PlayerInfo>
    ) -> Option<Vec<JsonApiStatusPlayer>> {
        let settings = sync_settings.read().await;
        let permissions = &settings.json_api.tokens[token];

        if !permissions.contains("Status/Players") {
            return None
        }

        let mut players: Vec<JsonApiStatusPlayer> = Vec::new();
        for (profile_id, cs) in clients {
            let id = if !permissions.contains("Status/Players/ID") { None } else {
                Some(profile_id.to_string())
            };

            let client = cs.data.read().await;
            let name = if !permissions.contains("Status/Players/Name") { None } else {
                Some(client.name.to_string())
            };
            let stage = if !permissions.contains("Status/Players/Stage") { None } else {
                match &client.last_game_packet {
                    Some(Packet { data: PacketData::Game { stage, .. }, .. }) => {
                        if stage == "" { None }
                        else { Some(stage.to_string()) }
                    },
                    _ => None,
                }
            };
            let scenario = if !permissions.contains("Status/Players/Scenario") { None } else {
                match &client.last_game_packet {
                    Some(Packet { data: PacketData::Game { scenario_num, .. }, .. }) => {
                        if *scenario_num == -1i8 { None }
                        else { Some(*scenario_num) }
                    },
                    _ => None,
                }
            };
            let costume = if !permissions.contains("Status/Players/Costume") { None } else {
                match &client.costume {
                    Some(cost) => Some(JsonApiStatusPlayerCostume {
                        body : cost.body_name.to_string(),
                        cap  : cost.cap_name.to_string(),
                    }),
                    _ => None,
                }
            };
            let ipv4 = if !permissions.contains("Status/Players/IPv4") { None } else { client.ipv4 };
            drop(client);

            let player = JsonApiStatusPlayer {
                id       : id,
                name     : name,
                stage    : stage,
                scenario : scenario,
                costume  : costume,
                ipv4     : ipv4,
            };
            players.push(player);
        }
        Some(players)
    }
}


#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct JsonApiStatusPlayerCostume {
    body : String,
    cap  : String,
}
