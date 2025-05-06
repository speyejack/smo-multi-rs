use crate::{
    cmds::{
        console::{BanCommand, FlipCommand, ScenarioCommand, ShineArg, TagCommand, UdpCommand, UnbanCommand},
        Command, ConsoleCommand, ExternalCommand, PlayerCommand, ServerWideCommand, ShineCommand,
    },
    guid::Guid,
    lobby::LobbyView,
    net::GameMode,
    player_holder::PlayerSelect,
    settings::{load_settings, save_settings},
    stages::Stages,
    types::{Result, SMOError},
};
use clap::Parser;
use std::{io::Write, time::Duration};
use tokio::{select, sync::oneshot};

// Call this console
#[derive(Parser, Debug)]
pub struct Cli {
    #[clap(subcommand)]
    pub cmd: ConsoleCommand,
}

pub struct Console {
    view: LobbyView,
}

impl Console {
    pub fn new(view: LobbyView) -> Self {
        Self { view }
    }

    pub async fn loop_read_commands(mut self) -> Result<()> {
        loop {
            // let command_result = parse_command(&mut to_coord).await;
            let command_result = select! {
                result = Console::read_input()=> {
                    result
                },
                exit_cmd = self.view.get_server_recv().recv() => {
                    match exit_cmd? {
                        ServerWideCommand::Shutdown => break Ok(())
                    }
                }

            };

            if let Err(e) = &command_result {
                println!("{}", e);
                continue;
            }

            let command_result = self.process_command(command_result.unwrap()).await;

            match command_result {
                Ok(s) => println!("{}", s),
                Err(e) => println!("Error processing command: {}", e),
            }
        }
    }

    pub async fn process_command(&mut self, cli: Cli) -> Result<String> {
        let reply_str = match cli.cmd {
            ConsoleCommand::SendAll { force, stage } => {
                let players: PlayerSelect<Guid> = PlayerSelect::AllPlayers;
                let players = players.into_guid_vec(&self.view)?;

                let actual_stage = Stages::input2stage(&stage);
                let actual_stage = match (actual_stage, force) {
                    (Some(s), _) => s,
                    (None, true) => stage.clone(),
                    (None, false) => {
                        return Err(SMOError::InvalidConsoleArg(
                            "Invalid stage name.".to_string(),
                        ))
                    }
                };

                self.request_comm(ExternalCommand::Player {
                    players,
                    command: PlayerCommand::Send {
                        stage: actual_stage,
                        id: "".to_string(),
                        scenario: -1,
                    },
                })
                .await?;
                format!("Sent players to {}:-1", stage)
            }
            ConsoleCommand::Send {
                force,
                stage,
                id,
                scenario,
                players,
            } => {
                let players: PlayerSelect<String> = (&players[..]).into();
                let players = players.into_guid_vec(&self.view).await?;

                let actual_stage = Stages::input2stage(&stage);
                let actual_stage = match (actual_stage, force) {
                    (Some(s), _) => s,
                    (None, true) => stage.clone(),
                    (None, false) => {
                        return Err(SMOError::InvalidConsoleArg(
                            "Invalid stage name.".to_string(),
                        ))
                    }
                };

                self.request_comm(ExternalCommand::Player {
                    players,
                    command: PlayerCommand::Send {
                        stage: actual_stage,
                        id,
                        scenario,
                    },
                })
                .await?;
                format!("Sent players to {}:{}", stage, scenario)
            }
            ConsoleCommand::Ban(subcmd) => match subcmd {
                BanCommand::List => {
                    let settings = self.view.get_lobby().settings.read().await;
                    let mut list = Vec::new();
                    list.push("BanList: ".to_string());
                    list.push(if settings.ban_list.enabled { "enabled" } else { "disabled" }.to_string());
                    if !settings.ban_list.ip_addresses.is_empty() {
                        list.push("\nBanned IPv4 addresses:".to_string());
                        for ip in settings.ban_list.ip_addresses.iter() {
                            list.push("\n- ".to_string());
                            list.push(ip.to_string());
                        }
                    }
                    if !settings.ban_list.players.is_empty() {
                        list.push("\nBanned profile IDs:".to_string());
                        for guid in settings.ban_list.players.iter() {
                            list.push("\n- ".to_string());
                            list.push(guid.to_string());
                        }
                    }
                    if !settings.ban_list.stages.is_empty() {
                        list.push("\nBanned stages:".to_string());
                        for stage in settings.ban_list.stages.iter() {
                            list.push("\n- ".to_string());
                            list.push(stage.to_string());
                        }
                    }
                    if !settings.ban_list.game_modes.is_empty() {
                        list.push("\nBanned gamemodes:".to_string());
                        for game_mode in settings.ban_list.game_modes.iter() {
                            list.push("\n- ".to_string());
                            list.push(GameMode::from_i8(*game_mode).to_string());
                        }
                    }
                    list.join("")
                },
                BanCommand::Enable => {
                    // update settings
                    let mut settings = self.view.get_mut_settings().write().await;
                    settings.ban_list.enabled = true;
                    save_settings(&settings)?;
                    drop(settings);

                    "BanList enabled.".to_string()
                },
                BanCommand::Disable => {
                    // update settings
                    let mut settings = self.view.get_mut_settings().write().await;
                    settings.ban_list.enabled = false;
                    save_settings(&settings)?;
                    drop(settings);

                    "BanList disabled.".to_string()
                },
                BanCommand::Player { players } => {
                    let players: PlayerSelect<String> = (&players[..]).into();
                    let players = players.into_guid_vec(&self.view).await?;

                    // get player data for banned players
                    let lobby = &self.view.get_lobby();
                    let guids = players.get_guids(lobby);
                    let ips   = players.get_ipv4s(lobby);
                    let names = players.get_names(lobby);

                    // update settings
                    let mut settings = self.view.get_mut_settings().write().await;
                    settings.ban_list.ip_addresses = settings
                        .ban_list
                        .ip_addresses
                        .union(&ips)
                        .copied()
                        .collect();
                    settings.ban_list.players = settings
                        .ban_list
                        .players
                        .union(&guids)
                        .copied()
                        .collect();
                    save_settings(&settings)?;
                    drop(settings);

                    // crash connected players
                    self.request_comm(ExternalCommand::Player {
                        players : players,
                        command : PlayerCommand::Crash {},
                    }).await?;

                    "Banned players: ".to_string() + &Vec::from_iter(names).join(", ")
                },
                BanCommand::Profile { profile_id } => {
                    // get connected players
                    let lobby = &self.view.get_lobby();
                    let guids: Vec<Guid> = lobby.players.iter().filter(|x| x.key() == &profile_id).map(|x| *x.key()).collect();
                    let players: PlayerSelect<Guid> = guids.into();
                    let players = players.into_guid_vec(&self.view).unwrap();

                    // update settings
                    let mut settings = self.view.get_mut_settings().write().await;
                    settings.ban_list.players.insert(profile_id);
                    save_settings(&settings)?;
                    drop(settings);

                    // crash connected players
                    self.request_comm(ExternalCommand::Player {
                        players : players,
                        command : PlayerCommand::Crash {},
                    }).await?;

                    "Banned profile: ".to_string() + &profile_id.to_string()
                },
                BanCommand::IP { ipv4 } => {
                    // get connected players
                    let lobby = &self.view.get_lobby();
                    let guids: Vec<Guid> = lobby.players.iter().filter(|x| x.value().ipv4 == Some(ipv4)).map(|x| *x.key()).collect();
                    let players: PlayerSelect<Guid> = guids.into();
                    let players = players.into_guid_vec(&self.view).unwrap();

                    // update settings
                    let mut settings = self.view.get_mut_settings().write().await;
                    settings.ban_list.ip_addresses.insert(ipv4);
                    save_settings(&settings)?;
                    drop(settings);

                    // crash connected players
                    self.request_comm(ExternalCommand::Player {
                        players : players,
                        command : PlayerCommand::Crash {},
                    }).await?;

                    "Banned ip: ".to_string() + &ipv4.to_string()
                },
                BanCommand::Stage { stage } => {
                    if Stages::input2stage(&stage).is_none() {
                        "Invalid stage name!".to_string()
                    } else {
                        let stages = Stages::stages_by_input(&stage);

                        // update settings
                        let mut settings = self.view.get_mut_settings().write().await;
                        for s in stages.iter() {
                            settings.ban_list.stages.insert(s.to_string());
                        }
                        save_settings(&settings)?;
                        drop(settings);

                        "Banned stages: ".to_string() + &stages.join(", ")
                    }
                },
                BanCommand::GameMode { game_mode } => {
                    // update settings
                    let mut settings = self.view.get_mut_settings().write().await;
                    settings.ban_list.game_modes.insert(GameMode::to_i8(game_mode));
                    save_settings(&settings)?;
                    drop(settings);

                    "Banned gamemode: ".to_string() + &game_mode.to_string()
                },
            },
            ConsoleCommand::Unban(subcmd) => match subcmd {
                UnbanCommand::Profile { profile_id } => {
                    // update settings
                    let mut settings = self.view.get_mut_settings().write().await;
                    settings.ban_list.players.remove(&profile_id);
                    save_settings(&settings)?;
                    drop(settings);

                    "Unbanned profile: ".to_string() + &profile_id.to_string()
                },
                UnbanCommand::IP { ipv4 } => {
                    // update settings
                    let mut settings = self.view.get_mut_settings().write().await;
                    settings.ban_list.ip_addresses.remove(&ipv4);
                    save_settings(&settings)?;
                    drop(settings);

                    "Unbanned ip: ".to_string() + &ipv4.to_string()
                },
                UnbanCommand::Stage { stage } => {
                    if Stages::input2stage(&stage).is_none() {
                        "Invalid stage name!".to_string()
                    } else {
                        let stages = Stages::stages_by_input(&stage);

                        // update settings
                        let mut settings = self.view.get_mut_settings().write().await;
                        for s in stages.iter() {
                            settings.ban_list.stages.remove(s);
                        }
                        save_settings(&settings)?;
                        drop(settings);

                        "Unbanned stages: ".to_string() + &stages.join(", ")
                    }
                },
                UnbanCommand::GameMode { game_mode } => {
                    // update settings
                    let mut settings = self.view.get_mut_settings().write().await;
                    settings.ban_list.game_modes.remove(&GameMode::to_i8(game_mode));
                    save_settings(&settings)?;
                    drop(settings);

                    "Unbanned gamemode: ".to_string() + &game_mode.to_string()
                },
            },
            ConsoleCommand::Crash { players } => {
                let players: PlayerSelect<String> = (&players[..]).into();
                let players = players.into_guid_vec(&self.view).await?;

                self.request_comm(ExternalCommand::Player {
                    players,
                    command: PlayerCommand::Crash {},
                })
                .await?
            }
            ConsoleCommand::Rejoin { players } => {
                let players: PlayerSelect<String> = (&players[..]).into();
                let players = players.into_guid_vec(&self.view).await?;

                self.request_comm(ExternalCommand::Player {
                    players,
                    command: PlayerCommand::Disconnect {},
                })
                .await?;
                "Rejoined players".to_string()
            }
            ConsoleCommand::Scenario(scenario) => match scenario {
                ScenarioCommand::Merge { enabled } => match enabled {
                    Some(to_enabled) => {
                        let mut settings = self.view.get_mut_settings().write().await;
                        settings.scenario.merge_enabled = to_enabled;
                        save_settings(&settings)?;
                        drop(settings);
                        if to_enabled {
                            "Enabled scenario merge"
                        } else {
                            "Disabled scenario merge"
                        }
                        .to_string()
                    }
                    None => {
                        let settings = self.view.get_mut_settings().read().await;
                        let is_enabled = settings.scenario.merge_enabled;
                        format!("Scenario merging is {}", is_enabled)
                    }
                },
            },
            ConsoleCommand::Tag(tag) => match tag {
                TagCommand::Time {
                    player,
                    minutes,
                    seconds,
                } => {
                    if seconds >= 60 {
                        return Err(SMOError::InvalidConsoleArg(
                            "Invalid number of seconds".to_string(),
                        ));
                    }
                    let players: PlayerSelect<String> = (&[player][..]).into();
                    let players = players.into_guid_vec(&self.view).await?;

                    self.request_comm(ExternalCommand::Player {
                        players,
                        command: PlayerCommand::Tag {
                            time: Some((minutes, seconds)),
                            is_seeking: None,
                        },
                    })
                    .await?;
                    format!("Set time for players to {}:{}", minutes, seconds)
                }
                TagCommand::Seeking { player, is_seeking } => {
                    let players: PlayerSelect<String> = (&[player][..]).into();
                    let players = players.into_guid_vec(&self.view).await?;

                    self.request_comm(ExternalCommand::Player {
                        players,
                        command: PlayerCommand::Tag {
                            time: None,
                            is_seeking: Some(is_seeking),
                        },
                    })
                    .await?;
                    format!("Changed is_seeking state to {}", is_seeking)
                }
                TagCommand::Start { countdown, seekers } => {
                    let seeker_ids: PlayerSelect<String> = (&seekers[..]).into();
                    let hiders = (!(seeker_ids.clone())).into_guid_vec(&self.view).await?;
                    let seekers = seeker_ids.into_guid_vec(&self.view).await?;

                    tokio::time::sleep(Duration::from_secs(countdown.into())).await;

                    self.request_comm(ExternalCommand::Player {
                        players: seekers,
                        command: PlayerCommand::Tag {
                            time: Some((0, 0)),
                            is_seeking: Some(true),
                        },
                    })
                    .await?;

                    self.request_comm(ExternalCommand::Player {
                        players: hiders,
                        command: PlayerCommand::Tag {
                            time: Some((0, 0)),
                            is_seeking: Some(false),
                        },
                    })
                    .await?
                }
            },
            ConsoleCommand::MaxPlayers { player_count } => {
                let mut settings = self.view.get_mut_settings().write().await;
                settings.server.max_players = player_count;
                save_settings(&settings)?;
                drop(settings);

                let players: PlayerSelect<Guid> = PlayerSelect::AllPlayers;
                let players = players.into_guid_vec(&self.view)?;

                self.request_comm(ExternalCommand::Player {
                    players,
                    command: PlayerCommand::Disconnect {},
                })
                .await?
            }
            ConsoleCommand::List => {
                let players: Vec<_> = self
                    .view
                    .get_lobby()
                    .names
                    .0
                    .read()
                    .await
                    .iter()
                    .map(|x| format!("{} ({})", x.0, x.1))
                    .collect();

                format!("List: \n\t{}", players.join("\n\t"))
            }
            ConsoleCommand::Flip(flip) => match flip {
                FlipCommand::List => {
                    let settings = self.view.get_mut_settings().write().await;
                    let player_str: Vec<String> = settings
                        .flip
                        .players
                        .iter()
                        .map(ToString::to_string)
                        .collect();
                    drop(settings);
                    format!("User ids: {}", &player_str.join(", "))
                }
                FlipCommand::Add { player } => {
                    let mut settings = self.view.get_mut_settings().write().await;
                    settings.flip.players.insert(player);
                    save_settings(&settings)?;
                    drop(settings);
                    format!("Added {} to flipped players", player)
                }
                FlipCommand::Remove { player } => {
                    let mut settings = self.view.get_mut_settings().write().await;
                    let was_removed = settings.flip.players.remove(&player);
                    save_settings(&settings)?;
                    drop(settings);
                    match was_removed {
                        true => format!("Removed {} to flipped players", player),
                        false => format!("User {} wasn't in the flipped players list", player),
                    }
                }
                FlipCommand::Set { is_flipped } => {
                    let mut settings = self.view.get_mut_settings().write().await;
                    settings.flip.enabled = is_flipped;
                    save_settings(&settings)?;
                    if is_flipped {
                        "Enabled player flipping".to_string()
                    } else {
                        "Disabled player flipping".to_string()
                    }
                }
                FlipCommand::Pov { value } => {
                    let mut settings = self.view.get_mut_settings().write().await;
                    settings.flip.pov = value;
                    save_settings(&settings)?;
                    format!("Point of view set to {}", value)
                }
            },
            ConsoleCommand::Shine(shine) => match shine {
                ShineArg::List => {
                    let mut out = "Shines: ".to_string();

                    let shines = self.view.get_lobby().shines.read().await;
                    out += &shines
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ");

                    let settings = self.view.get_lobby().settings.read().await;
                    if settings.shines.excluded.len() > 0 {
                        out += "\nExcluded Shines: ";
                        out += &settings.shines.excluded
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", ");
                    }

                    out
                }
                ShineArg::Clear => {
                    self.request_comm(ExternalCommand::Shine {
                        command: ShineCommand::Clear,
                    })
                    .await?
                }
                ShineArg::Sync => {
                    self.request_comm(ExternalCommand::Shine {
                        command: ShineCommand::Sync,
                    })
                    .await?
                }
                ShineArg::Send { id, player } => {
                    let players: PlayerSelect<String> = (&[player][..]).into();
                    let players = players.into_guid_vec(&self.view).await?;

                    self.request_comm(ExternalCommand::Player {
                        players,
                        command: PlayerCommand::SendShine { id },
                    })
                    .await?
                }
                ShineArg::Set { should_sync } => {
                    let mut settings = self.view.get_mut_settings().write().await;
                    settings.shines.enabled = should_sync;
                    save_settings(&settings)?;

                    if should_sync {
                        "Enabled shine sync"
                    } else {
                        "Disabled shine sync"
                    }
                    .to_string()
                }
                ShineArg::Include { id } => {
                    let mut settings = self.view.get_mut_settings().write().await;
                    settings.shines.excluded.remove(&id);
                    save_settings(&settings)?;
                    drop(settings);

                    format!("No longer exclude shine {} from syncing", id)
                }
                ShineArg::Exclude { id } => {
                    let mut settings = self.view.get_mut_settings().write().await;
                    settings.shines.excluded.insert(id);
                    save_settings(&settings)?;
                    drop(settings);

                    format!("Exclude shine {} from syncing", id)
                }
            },
            ConsoleCommand::Udp(udpcmd) => match udpcmd {
                UdpCommand::Init { player: _ } => unimplemented!("Udp is being phased out"),
                UdpCommand::Auto { should_auto } => {
                    let mut settings = self.view.get_mut_settings().write().await;
                    settings.udp.initiate_handshake = should_auto;
                    drop(settings);

                    if should_auto {
                        "Enabled auto udp handshake"
                    } else {
                        "Disabled auto udp handshake"
                    }
                    .to_string()
                }
            },
            ConsoleCommand::LoadSettings => {
                let mut settings = self.view.get_mut_settings().write().await;
                let new_settings = load_settings()?;
                *settings = new_settings;
                "Loaded settings.json".to_string()
            }
            ConsoleCommand::Restart => {
                self.view
                    .get_server_send()
                    .send(ServerWideCommand::Shutdown)?;
                "Restarting server".to_string()
            }
        };

        Ok(reply_str)
    }

    pub async fn request_comm(&self, command: ExternalCommand) -> Result<String> {
        let (sender, recv) = oneshot::channel();

        self.view
            .get_lobby()
            .to_coord
            .send(Command::External(command, sender))
            .await?;

        let result_str = recv.await?;
        let reply_str = result_str?;

        Ok(reply_str)
    }

    pub async fn read_input() -> Result<Cli> {
        let task = tokio::task::spawn_blocking(|| async { Self::get_input() });
        let cli: Cli = tokio::join!(task).0?.await?;
        Ok(cli)
    }

    pub fn get_input() -> Result<Cli> {
        let mut input = "> ".to_string();

        std::io::stdout().flush()?;
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim().split(' ');
        let cli = Cli::try_parse_from(input)?;

        Ok(cli)
    }
}
