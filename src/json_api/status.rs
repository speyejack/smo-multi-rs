use serde::Serialize;
use serde_json::Value;

use crate::json_api::{JsonApiStatusPlayer, JsonApiStatusSettings};
use crate::lobby::LobbyView;

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub(in crate::json_api) struct JsonApiStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    players: Option<Vec<JsonApiStatusPlayer>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    settings: Option<Value>,
}

impl JsonApiStatus {
    pub async fn create(view: &LobbyView, token: &String) -> JsonApiStatus {
        JsonApiStatus {
            players: JsonApiStatusPlayer::create(view, token).await,
            settings: JsonApiStatusSettings::create(view, token).await,
        }
    }
}
