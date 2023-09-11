use tokio::process::Command;

use crate::config::BackupConfig;

pub fn restore(backup: &BackupConfig) -> Command {
    let mut cmd = Command::new("restic");

    cmd.arg("-r")
        .arg(&backup.repo)
        .args(["--verbose", "restore", "latest"]);

    cmd
}

pub fn backup(backup: &BackupConfig) -> Command {
    let mut cmd = Command::new("restic");

    cmd.arg("-r")
        .arg(&backup.repo)
        .args(["--verbose", "backup"])
        .arg(&backup.src);

    cmd
}
