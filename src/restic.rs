use std::{collections::HashMap, process::Stdio};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use tokio::process::Command;

use crate::{
    config::{BackupConfig, RestoreConfig},
    log::logPrint,
};

#[derive(Debug, Clone, Deserialize)]
struct Snapshot {
    time: DateTime<Utc>,
}

pub trait ResticConfig {
    fn environments(&self) -> &Option<HashMap<String, String>>;
    fn repo(&self) -> &str;
}

impl ResticConfig for BackupConfig {
    fn environments(&self) -> &Option<HashMap<String, String>> {
        &self.environments
    }

    fn repo(&self) -> &str {
        &self.repo
    }
}

impl ResticConfig for RestoreConfig {
    fn environments(&self) -> &Option<HashMap<String, String>> {
        &self.environments
    }

    fn repo(&self) -> &str {
        &self.repo
    }
}

pub fn build_restic_command(config: &impl ResticConfig) -> Command {
    let mut cmd = Command::new("restic");

    if let Some(env) = config.environments() {
        for (k, v) in env {
            cmd.env(k, v);
        }
    }

    cmd.arg("-r").arg(config.repo());
    cmd
}

pub async fn get_latest_snapshot_time(config: &BackupConfig) -> Option<DateTime<Utc>> {
    let mut cmd = build_restic_command(config);

    logPrint!(
        "supervisor",
        "Getting latest snapshot time on {}",
        config.repo
    );

    let snapshots: Vec<Snapshot> = serde_json::from_slice(
        &cmd.args(["snapshots", "--json", "--latest", "1"])
            .arg("--path")
            .arg(&config.src)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .ok()?
            .wait_with_output()
            .await
            .ok()?
            .stdout,
    )
    .ok()?;

    snapshots.into_iter().next().map(|s| s.time)
}
