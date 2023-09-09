use std::{collections::HashMap, path::PathBuf};

use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub backups: Option<Vec<BackupConfig>>,
    pub app: AppConfig,
    pub update: Option<UpdateConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppConfig {
    pub image: String,
    pub args: Option<Vec<String>>,
    pub volumes: Option<Vec<String>>,
    pub environments: Option<HashMap<String, String>>,
    pub cap_add: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BackupConfig {
    pub repo: String,
    pub src: PathBuf,
    pub interval: Interval,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UpdateConfig {
    pub interval: Interval,
}

#[derive(Display, EnumString, Debug, Clone, SerializeDisplay, DeserializeFromStr)]
#[strum(serialize_all = "snake_case")]
pub enum Interval {
    Hourly,
    Daily,
    Weekly,
    Monthly,
}
