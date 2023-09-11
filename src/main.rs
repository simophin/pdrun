mod backup;
mod config;
mod image_info;
mod process;
mod runner;

use std::{
    future::pending,
    io::BufReader,
    path::PathBuf,
    process::{ExitCode, ExitStatus},
    time::{Duration, Instant},
};

use anyhow::{bail, Context};
use async_shutdown::Shutdown;
use chrono::{DateTime, Utc};
use clap::Parser;
use config::{AppConfig, BackupConfig, UpdateConfig};
use futures::never;
use tokio::{
    select,
    signal::ctrl_c,
    task::{spawn_local, LocalSet},
    time::{sleep, sleep_until},
};

use crate::process::Process;

/// A CLI tool to run your podman container with backup and auto update
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to the config file
    config: PathBuf,
}

fn main() -> anyhow::Result<ExitCode> {
    let _ = dotenvy::dotenv();
    env_logger::init();

    let Cli {
        config: config_path,
    } = Cli::parse();

    let config = std::fs::File::open(&config_path)
        .with_context(|| format!("Opening {}", config_path.display()))?;

    let config: config::Config =
        serde_yaml::from_reader(BufReader::new(config)).context("Reading config file")?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("to build a runtime");

    let status: u8 = LocalSet::new()
        .block_on(&rt, async move {
            let shutdown = Shutdown::new();
            spawn_local(monitor_ctrl_c(shutdown.clone()));
            run(config, shutdown).await
        })?
        .code()
        .unwrap_or(1)
        .try_into()
        .unwrap_or(1);

    Ok(ExitCode::from(status))
}

async fn restore(backup: &BackupConfig, shutdown: Shutdown) -> anyhow::Result<()> {
    if backup.src.exists() {
        log::info!(
            target: "supervisor",
            "Backup directory {} exists, skipping restore",
            backup.src.display()
        );
        return Ok(());
    }

    let mut process = Process::new("restore", backup::restore(backup), shutdown)
        .context("Starting restoring process")?;

    process
        .wait()
        .await
        .context("Waiting for restoring process")?;

    Ok(())
}

async fn init_pull_image(
    app: &AppConfig,
    update: &UpdateConfig,
    shutdown: Shutdown,
) -> anyhow::Result<Duration> {
    // Read last image creation time
    let image_creation_time = image_info::image_creation_time(&app.image).await.ok();
    let now = Utc::now();

    match image_creation_time {
        Some(last) if last + update.interval < now => {
            log::info!(target: "supervisor", "Skipped updating image");
        }

        _ => {
            log::info!(target: "supervisor", "Updating image");
            let mut process = Process::new("pull_image", runner::pull_image(&app), shutdown)
                .context("Starting update process")?;

            process.wait().await.context("Waiting for update process")?;

            image_info::image_creation_time(&app.image)
                .await
                .context("Reading image creation time")?;
        }
    };

    Ok((now + update.interval)
        .signed_duration_since(now)
        .to_std()
        .unwrap())
}

fn next_backup_time(backup: &BackupConfig, last_backup: Option<DateTime<Utc>>) -> Duration {
    let now = Utc::now();

    match last_backup {
        Some(last) if last + backup.interval < now => {
            (last + backup.interval).signed_duration_since(now)
        }
        _ => (now + backup.interval).signed_duration_since(now),
    }
    .to_std()
    .unwrap()
}

async fn sleep_until_or_forever(until: Option<Instant>) {
    match until {
        Some(until) => sleep_until(until.into()).await,
        None => pending().await,
    }
}

async fn monitor_ctrl_c(shutdown: Shutdown) {
    let _ = ctrl_c().await;
    log::info!(target: "supervisor", "Received Ctrl-C, shutting down");
    shutdown.shutdown();
}

async fn run(config: config::Config, shutdown: Shutdown) -> anyhow::Result<ExitStatus> {
    let config::Config {
        backup,
        app,
        update,
    } = config;
    let update = update.unwrap_or_default();

    if let Some(backup) = &backup {
        restore(backup, shutdown.clone()).await?;
    }

    let mut next_update = Instant::now() + init_pull_image(&app, &update, shutdown.clone()).await?;
    log::info!(target: "supervisor", "Next image update time is: {next_update:?}");

    let mut process =
        Process::new("app", runner::run_app(&app), shutdown).context("Starting app process")?;

    let mut last_backup = None;
    let mut next_backup = backup
        .as_ref()
        .map(|c| Instant::now() + next_backup_time(c, last_backup));

    loop {
        select! {
            _ = sleep_until_or_forever(next_backup) => {
                log::info!(target: "supervisor", "Started backup");
            }

            _ = sleep_until(next_update.into()) => {
                log::info!(target: "supervisor", "Started updating");
            }
        }
    }

    process.wait().await.context("Waiting for app process")
}
