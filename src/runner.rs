use std::process::{ExitCode, Stdio};

use anyhow::Context;
use tokio::{
    process::{Child, Command},
    runtime::Runtime,
    select, signal,
    task::JoinHandle,
};

use super::config::AppConfig;

pub fn run_app(
    rt: &Runtime,
    config: AppConfig,
) -> anyhow::Result<JoinHandle<anyhow::Result<ExitCode>>> {
    let mut cmd = Command::new("docker");

    cmd.arg("run");

    let AppConfig {
        image,
        args,
        volumes,
        cap_add,
        environments,
    } = config;

    if let Some(envs) = environments {
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
        .spawn()
        .context("Starting container")?;

    Ok(rt.spawn(run_app_async(child)))
}

async fn run_app_async(mut child: Child) -> anyhow::Result<ExitCode> {
    let status = select! {
        _ = signal::ctrl_c() => {
            log::info!("Ctrl-C received, stopping container");
            child.kill().await.context("Killing container")?;
            child.wait().await
        }

        status = child.wait() => status,
    };

    if let Some(code) = status.context("Waiting for container to finish")?.code() {
        Ok(ExitCode::from(code.try_into().unwrap_or(1)))
    } else {
        Ok(ExitCode::FAILURE)
    }
}
