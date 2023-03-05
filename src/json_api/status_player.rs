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
    position: Option<Vector3>,

    #[serde(skip_serializing_if = "Option::is_none")]
    tagged: Option<bool>,

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

            let id = permissions
                .contains("Status/Players/ID")
                .then(|| profile_id.to_string());

            let client = client_ref.value();
            let name = permissions
                .contains("Status/Players/Name")
                .then(|| client.name.to_string());

            let kingdom = permissions
                .contains("Status/Players/Kingdom")
                .then(|| match &client.last_game_packet {
                    Some(Packet {
                        data: PacketData::Game { stage, .. },
                        ..
                    }) => Stages::stage2kingdom(stage),
                    _ => None,
                })
                .flatten();

            let stage = permissions
                .contains("Status/Players/Stage")
                .then(|| match &client.last_game_packet {
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
                })
                .flatten();

            let scenario = permissions
                .contains("Status/Players/Scenario")
                .then(|| match &client.last_game_packet {
                    Some(Packet {
                        data: PacketData::Game { scenario_num, .. },
                        ..
                    }) => (*scenario_num != -1).then_some(*scenario_num),
                    _ => None,
                })
                .flatten();

            let costume = permissions
                .contains("Status/Players/Costume")
                .then_some(())
                .and(client.costume.as_ref())
                .map(|cost| JsonApiStatusPlayerCostume {
                    body: cost.body_name.to_string(),
                    cap: cost.cap_name.to_string(),
                });

            let position = permissions
                .contains("Status/Players/Position")
                .then_some(client.last_position);

            let ipv4 = permissions
                .contains("Status/Players/IPv4")
                .then_some(client.ipv4)
                .flatten();

            let tagged = permissions
                .contains("Status/Players/Tagged")
                .then_some(client.is_seeking);

            let player = JsonApiStatusPlayer {
                id,
                name,
                kingdom,
                stage,
                scenario,
                position,
                costume,
                tagged,
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
