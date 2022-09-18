use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;


use crate::guid::Guid;
use crate::json_api::{ JsonApiStatusPlayer, JsonApiStatusSettings };
use crate::player_holder::PlayerInfo;
use crate::settings::SyncSettings;


#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub(in crate::json_api) struct JsonApiStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    players : Option<Vec<JsonApiStatusPlayer>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    settings : Option<Value>,
}


impl JsonApiStatus {
    pub async fn create(
        sync_settings : &SyncSettings,
        token         : &String,
        clients       : &HashMap<Guid, PlayerInfo>
    ) -> JsonApiStatus {
        JsonApiStatus {
            players  : JsonApiStatusPlayer::create(sync_settings, &token, clients).await,
            settings : JsonApiStatusSettings::create(sync_settings, &token).await,
        }
    }
}
