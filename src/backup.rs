use tokio::process::Command;

use crate::{config::BackupConfig, restic::build_restic_command};

pub fn restore(backup: &BackupConfig) -> Command {
    let mut cmd = build_restic_command(backup);
    cmd.args(["--verbose", "restore", "latest"])
        .arg("--target")
        .arg(&backup.src);

    cmd
}

pub fn backup(backup: &BackupConfig) -> Command {
    let mut cmd = build_restic_command(backup);

    cmd.args(["--verbose", "backup"]).arg(&backup.src);
    cmd
}
