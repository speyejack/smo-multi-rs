use serde::Serialize;
use std::net::IpAddr;

use crate::lobby::LobbyView;
use crate::net::{GameMode, Packet, PacketData};
use crate::stages::Stages;

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub(in crate::json_api) struct JsonApiStatusPlayer {
    #[serde(skip_serializing_if = "Option::is_none", rename = "ID")]
    id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    game_mode: Option<i8>, // display u4 (0..15) as (-1..14)

    #[serde(skip_serializing_if = "Option::is_none")]
    kingdom: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    stage: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    scenario: Option<i8>,

    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<JsonApiStatusPlayerPosition>,

    #[serde(skip_serializing_if = "Option::is_none")]
    rotation: Option<JsonApiStatusPlayerRotation>,

    #[serde(skip_serializing_if = "Option::is_none")]
    tagged: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    costume: Option<JsonApiStatusPlayerCostume>,

    #[serde(skip_serializing_if = "Option::is_none")]
    capture: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none", rename = "Is2D")]
    is_2d: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none", rename = "IPv4")]
    ipv4: Option<IpAddr>,
}

impl JsonApiStatusPlayer {
    pub async fn create(view: &LobbyView, token: &String) -> Option<Vec<JsonApiStatusPlayer>> {
        let permissions = &view.get_lobby().settings.read().await.json_api.tokens[token];

        if !permissions.contains("Status/Players") {
            return None;
        }

        let id_perm       = permissions.contains("Status/Players/ID");
        let name_perm     = permissions.contains("Status/Players/Name");
        let gamemode_perm = permissions.contains("Status/Players/GameMode");
        let kingdom_perm  = permissions.contains("Status/Players/Kingdom");
        let stage_perm    = permissions.contains("Status/Players/Stage");
        let scenario_perm = permissions.contains("Status/Players/Scenario");
        let costume_perm  = permissions.contains("Status/Players/Costume");
        let capture_perm  = permissions.contains("Status/Players/Capture");
        let position_perm = permissions.contains("Status/Players/Position");
        let rotation_perm = permissions.contains("Status/Players/Rotation");
        let is2d_perm     = permissions.contains("Status/Players/Is2D");
        let ipv4_perm     = permissions.contains("Status/Players/IPv4");
        let tagged_perm   = permissions.contains("Status/Players/Tagged");

        let mut players: Vec<JsonApiStatusPlayer> = Vec::new();
        for client_ref in view.get_lobby().players.iter() {
            let profile_id = client_ref.key();

            let id = id_perm.then(|| profile_id.to_string());

            let client = client_ref.value();
            let name = name_perm.then(|| client.name.to_string());
            let game_mode = gamemode_perm.then(|| ((GameMode::to_u8(client.game_mode) + 1) % 16) as i8 - 1);

            let kingdom = kingdom_perm
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
                .then(|| match &client.last_costume_packet {
                    Some(Packet {
                        data: PacketData::Costume(cost),
                        ..
                    }) => Some(JsonApiStatusPlayerCostume {
                        body: cost.body_name.to_string(),
                        cap: cost.cap_name.to_string(),
                    }),
                    _ => None,
                })
                .flatten();

            let capture = capture_perm
                .then(|| match &client.last_capture_packet {
                    Some(Packet {
                        data: PacketData::Capture { model },
                        ..
                    }) => Some(model.to_string()),
                    _ => None,
                })
                .flatten();

            let position = position_perm
                .then(|| match &client.last_player_packet {
                    Some(Packet {
                        data: PacketData::Player { pos, .. },
                        ..
                    }) => Some(JsonApiStatusPlayerPosition {
                        x: pos.x,
                        y: pos.y,
                        z: pos.z,
                    }),
                    _ => None,
                })
                .flatten();

            let rotation = rotation_perm
                .then(|| match &client.last_player_packet {
                    Some(Packet {
                        data: PacketData::Player { rot, .. },
                        ..
                    }) => Some(JsonApiStatusPlayerRotation {
                        w: rot.w,
                        x: rot.i,
                        y: rot.j,
                        z: rot.k,
                    }),
                    _ => None,
                })
                .flatten();

            let is_2d = is2d_perm
                .then(|| match &client.last_game_packet {
                    Some(Packet {
                        data: PacketData::Game { is_2d, .. },
                        ..
                    }) => Some(*is_2d),
                    _ => None,
                })
                .flatten();

            let ipv4 = ipv4_perm.then_some(client.ipv4).flatten();

            let tagged = tagged_perm.then_some(client.is_seeking).flatten();

            let player = JsonApiStatusPlayer {
                id,
                name,
                game_mode,
                kingdom,
                stage,
                scenario,
                position,
                rotation,
                costume,
                capture,
                is_2d,
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

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct JsonApiStatusPlayerPosition {
    x: f32,
    y: f32,
    z: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct JsonApiStatusPlayerRotation {
    w: f32,
    x: f32,
    y: f32,
    z: f32,
}
