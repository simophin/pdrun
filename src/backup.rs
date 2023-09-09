use std::process::Stdio;

use anyhow::Context;
use tokio::process::{Child, Command};

use crate::config::BackupConfig;

pub async fn restore(backup: &BackupConfig) -> anyhow::Result<Option<Child>> {
    if backup.src.exists() {
        log::info!(
            "Directory {} exists, skipping restore",
            backup.src.display()
        );

        return Ok(None);
    }

    let mut cmd = Command::new("restic");

    let child = cmd
        .arg("-r")
        .arg(&backup.repo)
        .args(["--verbose", "restore", "latest"])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context("Starting restore process")?;

    Ok(Some(child))
}
