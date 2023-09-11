use std::{collections::HashMap, ops::Add, path::PathBuf};

use chrono::{DateTime, Days, Months, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub backup: Option<BackupConfig>,
    pub app: AppConfig,
    pub update: Option<UpdateConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppConfig {
    pub image: String,
    pub args: Option<Vec<String>>,
    pub volumes: Option<Vec<String>>,
    pub ports: Option<Vec<String>>,
    pub network_mode: Option<NetworkMode>,
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

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            interval: Interval::Daily,
        }
    }
}

#[derive(Display, EnumString, Debug, Clone, SerializeDisplay, DeserializeFromStr, Copy)]
#[strum(serialize_all = "snake_case")]
pub enum Interval {
    Hourly,
    Daily,
    Weekly,
    Monthly,
}

impl Add<Interval> for DateTime<Utc> {
    type Output = Self;

    fn add(self, rhs: Interval) -> Self::Output {
        match rhs {
            Interval::Hourly => self.add(chrono::Duration::hours(1)),
            Interval::Daily => self.checked_add_days(Days::new(1)).unwrap(),
            Interval::Weekly => self.checked_add_days(Days::new(7)).unwrap(),
            Interval::Monthly => self.checked_add_months(Months::new(1)).unwrap(),
        }
    }
}

#[derive(Display, EnumString, Debug, Clone, SerializeDisplay, DeserializeFromStr)]
#[strum(serialize_all = "snake_case")]
pub enum NetworkMode {
    Host,
    Bridge,
}
