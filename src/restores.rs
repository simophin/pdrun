use tokio::process::Command;

use crate::{config::RestoreConfig, restic::build_restic_command};

pub fn restore(r: &RestoreConfig) -> Command {
    let mut cmd = build_restic_command(r);
    cmd.args(["--verbose", "restore", "latest"])
        .arg("--target")
        .arg(&r.dst);

    cmd
}
