use serde::Serialize;
use std::net::IpAddr;

use crate::coordinator::Coordinator;
use crate::lobby::LobbyView;
use crate::net::{Packet, PacketData};
use crate::stages::Stages;
use crate::types::Vector3;

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub(in crate::json_api) struct JsonApiStatusPlayer {
    #[serde(skip_serializing_if = "Option::is_none", rename = "ID")]
    id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    kingdom: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    stage: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    scenario: Option<i8>,

    #[serde(skip_serializing_if = "Option::is_none")]
    location: Option<Vector3>,

    #[serde(skip_serializing_if = "Option::is_none")]
    costume: Option<JsonApiStatusPlayerCostume>,

    #[serde(skip_serializing_if = "Option::is_none", rename = "IPv4")]
    ipv4: Option<IpAddr>,
}

impl JsonApiStatusPlayer {
    pub async fn create(view: &LobbyView, token: &String) -> Option<Vec<JsonApiStatusPlayer>> {
        let permissions = &view.get_lobby().settings.read().await.json_api.tokens[token];

        if !permissions.contains("Status/Players") {
            return None;
        }

        let mut players: Vec<JsonApiStatusPlayer> = Vec::new();
        for client_ref in view.get_lobby().players.iter() {
            let profile_id = client_ref.key();
            let id = if !permissions.contains("Status/Players/ID") {
                None
            } else {
                Some(profile_id.to_string())
            };

            let client = client_ref.value();
            let name = if !permissions.contains("Status/Players/Name") {
                None
            } else {
                Some(client.name.to_string())
            };
            let kingdom = if !permissions.contains("Status/Players/Kingdom") {
                None
            } else {
                match &client.last_game_packet {
                    Some(Packet {
                        data: PacketData::Game { stage, .. },
                        ..
                    }) => Stages::stage2kingdom(stage),
                    _ => None,
                }
            };
            let stage = if !permissions.contains("Status/Players/Stage") {
                None
            } else {
                match &client.last_game_packet {
                    Some(Packet {
                        data: PacketData::Game { stage, .. },
                        ..
                    }) => {
                        if stage.is_empty() {
                            None
                        } else {
                            Some(stage.to_string())
                        }
                    }
                    _ => None,
                }
            };
            let scenario = if !permissions.contains("Status/Players/Scenario") {
                None
            } else {
                match &client.last_game_packet {
                    Some(Packet {
                        data: PacketData::Game { scenario_num, .. },
                        ..
                    }) => {
                        if *scenario_num == -1 {
                            None
                        } else {
                            Some(*scenario_num)
                        }
                    }
                    _ => None,
                }
            };
            let costume = if !permissions.contains("Status/Players/Costume") {
                None
            } else {
                match &client.costume {
                    Some(cost) => Some(JsonApiStatusPlayerCostume {
                        body: cost.body_name.to_string(),
                        cap: cost.cap_name.to_string(),
                    }),
                    _ => None,
                }
            };
            let location = if !permissions.contains("Status/Players/Location") {
                None
            } else {
                Some(client.last_position)
            };
            let ipv4 = if !permissions.contains("Status/Players/IPv4") {
                None
            } else {
                client.ipv4
            };

            let player = JsonApiStatusPlayer {
                id,
                name,
                kingdom,
                stage,
                scenario,
                location,
                costume,
                ipv4,
            };
            players.push(player);
        }
        Some(players)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct JsonApiStatusPlayerCostume {
    body: String,
    cap: String,
}
