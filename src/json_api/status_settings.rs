use serde_json::{json, Map, Value};

use crate::lobby::LobbyView;

pub(in crate::json_api) struct JsonApiStatusSettings {}

impl JsonApiStatusSettings {
    pub async fn create(view: &LobbyView, token: &String) -> Option<Value> {
        let settings = view.get_lobby().settings.read().await;

        let permissions: Vec<String> = settings.json_api.tokens[token]
            .iter()
            .filter(|s| s.len() > 16 && &s[..16] == "Status/Settings/")
            .map(|s| s[16..].to_string())
            .collect();

        if permissions.len() == 0 {
            return None;
        }

        let mut has_results = false;
        let mut jsett = json!(&*settings);
        let mut result = Value::Object(Map::new());
        drop(settings);

        'outer: for perm in permissions {
            let mut node = &mut result;
            let mut sett = &mut jsett;
            let mut last = "";

            for key in perm.split("/") {
                if last != "" {
                    // traverse down the settings object
                    if let Value::Object(ref mut sett_map) = sett {
                        sett = &mut sett_map[&last.to_string()];
                    }
                }

                // key exists in settings?
                if let Value::Object(ref mut sett_map) = sett {
                    if !sett_map.contains_key(&key.to_string()) {
                        JsonApiStatusSettings::missing_setting(&perm);
                        continue 'outer;
                    }
                } else {
                    // key was already set to a concrete value by an earlier permission,
                    // meaning that it can't contain sub keys, because it isn't an object.
                    JsonApiStatusSettings::missing_setting(&perm);
                    continue 'outer;
                }

                if last != "" {
                    if let Value::Object(ref mut node_map) = node {
                        // create the sublayer
                        if !node_map.contains_key(&last.to_string()) {
                            let val = Value::Object(Map::new());
                            node_map.insert(last.to_string(), val);
                        }

                        // traverse down the output object
                        node = &mut node_map[&last.to_string()];
                    }
                }

                last = key;
            }

            // copy key with the actual value
            if let Value::Object(ref mut node_map) = node {
                if let Value::Object(ref mut sett_map) = sett {
                    let val = &sett_map[&last.to_string()];
                    node_map.insert(last.to_string(), json!(val));
                    has_results = true;
                }
            }
        }

        if !has_results {
            return None;
        }

        Some(result)
    }

    fn missing_setting(perm: &String) {
        tracing::warn!("Permission \"Status/Settings/{}\" doesn't exist on the Settings object. This is probably a misconfiguration in the settings.json", perm);
    }
}
