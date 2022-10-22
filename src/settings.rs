use std::{
    collections::HashSet,
    fmt::Display,
    fs::File,
    io::{BufReader, BufWriter},
    net::IpAddr,
    str::FromStr,
    sync::Arc,
};

use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::{
    guid::Guid,
    types::{Result, SMOError},
};

pub type SyncSettings = Arc<RwLock<Settings>>;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Settings {
    pub server: ServerSettings,
    pub flip: FlipSettings,
    pub scenario: ScenarioSettings,
    pub ban_list: BanListSettings,
    pub discord: DiscordSettings,
    pub persist_shines: PersistShine,
    pub udp: Udp,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ServerSettings {
    pub address: IpAddr,
    pub port: u16,
    pub max_players: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct FlipSettings {
    pub enabled: bool,
    pub players: HashSet<Guid>,
    pub pov: FlipPovSettings,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "PascalCase")]
#[clap(rename_all = "lower")]
pub enum FlipPovSettings {
    Both,
    Player,
    Others,
}

impl FromStr for FlipPovSettings {
    type Err = SMOError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        if s == "both" {
            Ok(Self::Both)
        } else if s == "self" || s == "players" {
            Ok(Self::Player)
        } else if s == "others" {
            Ok(Self::Others)
        } else {
            Err(SMOError::InvalidConsoleArg(
                "Invalid Flip POV Settings".to_string(),
            ))
        }
    }
}

impl Display for FlipPovSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlipPovSettings::Both => write!(f, "both"),
            FlipPovSettings::Player => write!(f, "self"),
            FlipPovSettings::Others => write!(f, "others"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ScenarioSettings {
    pub merge_enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct BanListSettings {
    pub enabled: bool,
    pub players: HashSet<Guid>,
    pub ips: HashSet<IpAddr>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DiscordSettings {
    pub token: Option<String>,
    pub prefix: String,
    pub log_channel: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PersistShine {
    pub enabled: bool,
    pub filename: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Udp {
    pub initiate_handshake: bool,
    pub base_port: u32,
    pub port_count: u32,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            address: "0.0.0.0".parse().unwrap(),
            port: 1027,
            max_players: 8,
        }
    }
}

impl Default for FlipSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            players: Default::default(),
            pov: Default::default(),
        }
    }
}

impl Default for ScenarioSettings {
    fn default() -> Self {
        Self {
            merge_enabled: false,
        }
    }
}

impl Default for BanListSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            players: Default::default(),
            ips: Default::default(),
        }
    }
}

impl Default for DiscordSettings {
    fn default() -> Self {
        Self {
            token: Default::default(),
            prefix: "$".to_string(),
            log_channel: Default::default(),
        }
    }
}

impl Default for PersistShine {
    fn default() -> Self {
        Self {
            enabled: false,
            filename: "./moons.json".into(),
        }
    }
}

impl Default for FlipPovSettings {
    fn default() -> Self {
        Self::Both
    }
}

impl Default for Udp {
    fn default() -> Self {
        Self {
            initiate_handshake: false,
            base_port: 0,
            port_count: 1,
        }
    }
}

pub fn load_settings() -> Result<Settings> {
    let file = File::open("./settings.json")?;
    let reader = BufReader::new(file);
    let settings = serde_json::from_reader(reader)?;
    tracing::debug!("Loading settings");

    Ok(settings)
}

pub fn save_settings(settings: &Settings) -> Result<()> {
    tracing::debug!("Saving settings");
    let file = File::create("./settings.json")?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, settings)?;
    Ok(())
}
