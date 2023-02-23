use clap::Parser;
use serde::Serialize;

use crate::console::Cli;
use crate::coordinator::Coordinator;

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub(in crate::json_api) struct JsonApiCommands {
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<String>,
}

impl JsonApiCommands {
    pub async fn process(
        coord: &mut Coordinator,
        token: &String,
        data: &Option<String>,
    ) -> JsonApiCommands {
        let settings = coord.settings.read().await;
        let permissions = &settings.json_api.tokens[token];

        // no permission in general
        if !permissions.contains("Commands") {
            return JsonApiCommands::result("Error: Missing Commands permission.".to_string());
        }

        if data.is_none() {
            return JsonApiCommands::result("Error: Invalid request - Data is missing".to_string());
        }

        let input = &data.as_deref().unwrap().to_string();
        drop(data);

        // help doesn't need permissions and is individualized to the token
        if input == "help" {
            return JsonApiCommands::result(format!(
                "Valid commands: {}",
                permissions
                    .into_iter()
                    .filter(|perm| perm.starts_with("Commands/"))
                    .map(|perm| perm.chars().skip(9).collect())
                    .collect::<Vec<String>>()
                    .join(", ")
                    .to_string()
            ));
        }

        let cmd: String = input.trim().split(' ').collect::<Vec<_>>()[0].to_string();

        // no specific permissions
        let perm = format!("Commands/{}", cmd);
        if !permissions.contains(&perm) {
            return JsonApiCommands::result(
                format!("Error: Missing {} permission.", perm).to_string(),
            );
        }

        drop(permissions);
        drop(settings);

        // execute command
        tracing::info!("{}", input.trim());
        let parsed = Cli::try_parse_from(format!("> {}", input.trim()).split(' '));
        match parsed {
            Ok(cli) => match coord.handle_console_cmd(cli.cmd).await {
                Ok(res) => {
                    tracing::info!("{}", res);
                    return JsonApiCommands::result(res);
                }
                Err(error) => {
                    tracing::error!("{}", error);
                    return JsonApiCommands::result(format!("{}", error).to_string());
                }
            },
            _ => {
                tracing::warn!("Invalid Command: {}", input.trim());
                return JsonApiCommands::result(
                    format!("Error: Invalid Command - {}", input.trim()).to_string(),
                );
            }
        }
    }

    pub fn result(str: String) -> JsonApiCommands {
        return JsonApiCommands { output: Some(str) };
    }
}
