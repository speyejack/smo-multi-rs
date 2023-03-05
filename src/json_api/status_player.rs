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

        let id_perm = permissions.contains("Status/Players/ID");
        let name_perm = permissions.contains("Status/Players/Name");
        let kingdom_per = permissions.contains("Status/Players/Kingdom");
        let stage_perm = permissions.contains("Status/Players/Stage");
        let scenario_perm = permissions.contains("Status/Players/Scenario");
        let costume_perm = permissions.contains("Status/Players/Costume");
        let position_perm = permissions.contains("Status/Players/Position");
        let ipv4_perm = permissions.contains("Status/Players/IPv4");
        let tagged_perm = permissions.contains("Status/Players/Tagged");

        let mut players: Vec<JsonApiStatusPlayer> = Vec::new();
        for client_ref in view.get_lobby().players.iter() {
            let profile_id = client_ref.key();

            let id = id_perm.then(|| profile_id.to_string());

            let client = client_ref.value();
            let name = name_perm.then(|| client.name.to_string());

            let kingdom = kingdom_per
                .then(|| match &client.last_game_packet {
                    Some(Packet {
                        data: PacketData::Game { stage, .. },
                        ..
                    }) => Stages::stage2kingdom(stage),
                    _ => None,
                })
                .flatten();

            let stage = stage_perm
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

            let scenario = scenario_perm
                .then(|| match &client.last_game_packet {
                    Some(Packet {
                        data: PacketData::Game { scenario_num, .. },
                        ..
                    }) => (*scenario_num != -1).then_some(*scenario_num),
                    _ => None,
                })
                .flatten();

            let costume = costume_perm
                .then_some(())
                .and(client.costume.as_ref())
                .map(|cost| JsonApiStatusPlayerCostume {
                    body: cost.body_name.to_string(),
                    cap: cost.cap_name.to_string(),
                });

            let position = position_perm.then_some(client.last_position);

            let ipv4 = ipv4_perm.then_some(client.ipv4).flatten();

            let tagged = tagged_perm.then_some(client.is_seeking);

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
