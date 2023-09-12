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

pub async fn get_latest_snapshot_time(
    config: &BackupConfig,
) -> anyhow::Result<Option<DateTime<Utc>>> {
    let mut cmd = Command::new("restic");

    let snapshots: Vec<Snapshot> = serde_json::from_slice(
        &cmd.args(["snapshots", "--json", "--latest", "1"])
            .arg("--path")
            .arg(&config.src)
            .arg("-r")
            .arg(&config.repo)
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
