use std::process::Stdio;

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use tokio::process::Command;

use crate::config::BackupConfig;

#[derive(Debug, Clone, Deserialize)]
struct Snapshot {
    time: DateTime<Utc>,
}

pub fn build_restic_command(config: &BackupConfig) -> Command {
    let mut cmd = Command::new("restic");

    if let Some(env) = &config.environments {
        for (k, v) in env {
            cmd.env(k, v);
        }
    }

    cmd.arg("-r").arg(&config.repo);
    cmd
}

pub async fn get_latest_snapshot_time(
    config: &BackupConfig,
) -> anyhow::Result<Option<DateTime<Utc>>> {
    let mut cmd = build_restic_command(config);

    let snapshots: Vec<Snapshot> = serde_json::from_slice(
        &cmd.args(["snapshots", "--json", "--latest", "1"])
            .arg("--path")
            .arg(&config.src)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .context("Starting restic")?
            .wait_with_output()
            .await
            .context("Running restic snapshots")?
            .stdout,
    )
    .context("Deserialize restic snapshots response")?;

    Ok(snapshots.into_iter().next().map(|s| s.time))
}
