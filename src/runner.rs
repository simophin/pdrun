use std::process::Stdio;

use anyhow::Context;
use tokio::process::{Child, Command};

use super::config::AppConfig;

pub fn run_app(config: &AppConfig) -> anyhow::Result<Child> {
    let mut cmd = Command::new("docker");

    cmd.arg("run");

    let AppConfig {
        image,
        args,
        volumes,
        cap_add,
        environments,
    } = config;

    if let Some(envs) = &environments {
        for (key, value) in envs {
            cmd.arg("-e").arg(format!("{key}={value}"));
        }
    }

    if let Some(volumes) = volumes {
        for volume in volumes {
            cmd.arg("-v").arg(volume);
        }
    }

    if let Some(cap_add) = cap_add {
        for cap in cap_add {
            cmd.arg("--cap-add").arg(cap);
        }
    }

    cmd.args(["--network", "host", "-it", "--rm"]);
    cmd.arg(image);

    if let Some(args) = args {
        cmd.args(args);
    }

    let child = cmd
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .context("Starting container")?;

    Ok(child)
}
