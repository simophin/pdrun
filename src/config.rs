use std::{collections::HashMap, fmt::Display, path::PathBuf, str::FromStr, time::Duration};

use chrono::{DateTime, Days, TimeZone};
use cron::Schedule;
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
    pub strategy: Option<BackupStrategy>,
    pub environments: Option<HashMap<String, String>>,
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

#[derive(
    Display, EnumString, Debug, Clone, SerializeDisplay, DeserializeFromStr, Copy, PartialEq, Eq,
)]
#[strum(serialize_all = "snake_case")]
pub enum BackupStrategy {
    StopApp,
    Live,
}

impl Default for BackupStrategy {
    fn default() -> Self {
        Self::StopApp
    }
}

#[derive(Debug, Clone, SerializeDisplay, DeserializeFromStr)]
pub enum Interval {
    Hourly,
    Daily,
    Weekly,
    Custom(Schedule),
}

impl FromStr for Interval {
    type Err = cron::error::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "hourly" => Ok(Self::Hourly),
            "daily" => Ok(Self::Daily),
            "weekly" => Ok(Self::Weekly),
            s => Ok(Self::Custom(s.parse()?)),
        }
    }
}

impl Display for Interval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Interval::Hourly => f.write_str("hourly"),
            Interval::Daily => f.write_str("daily"),
            Interval::Weekly => f.write_str("weekly"),
            Interval::Custom(s) => Display::fmt(s, f),
        }
    }
}

impl Interval {
    pub fn next<Tz>(&self, last: Option<DateTime<Tz>>, now: DateTime<Tz>) -> Option<Duration>
    where
        Tz: TimeZone,
        <Tz as TimeZone>::Offset: Copy,
    {
        let next = match self {
            Interval::Hourly => Some(last.unwrap_or_else(|| now) + chrono::Duration::hours(1)),
            Interval::Daily => last.unwrap_or_else(|| now).checked_add_days(Days::new(1)),
            Interval::Weekly => last.unwrap_or_else(|| now).checked_add_days(Days::new(7)),
            Interval::Custom(s) => s.after_owned(now).next(),
        };

        match next {
            Some(next) if next >= now => (next - now).to_std().ok(),
            Some(_) => Some(Duration::ZERO),
            None => None,
        }
    }
}

#[derive(Display, EnumString, Debug, Clone, SerializeDisplay, DeserializeFromStr)]
#[strum(serialize_all = "snake_case")]
pub enum NetworkMode {
    Host,
    Bridge,
}
