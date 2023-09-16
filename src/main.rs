mod backup;
mod config;
mod image_info;

mod log;
mod process;
mod restic;
mod restores;
mod runner;
mod tz;

use std::{
    future::pending,
    io::BufReader,
    path::PathBuf,
    process::{ExitCode, ExitStatus},
};

use anyhow::{bail, Context};
use async_shutdown::Shutdown;
use chrono::Utc;
use clap::Parser;
use config::{AppConfig, BackupConfig, RestoreConfig};
use restores::restore;
use runner::pull_image;
use tokio::{
    select,
    signal::ctrl_c,
    task::{spawn_local, LocalSet},
    time::{sleep_until, Instant},
};
use tz::current_timezone;

use crate::process::Process;
use log::logPrint;

/// A CLI tool to run your podman container with backup and auto update
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to the config file
    config: PathBuf,
}

fn main() -> anyhow::Result<ExitCode> {
    let _ = dotenvy::dotenv();

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
            run(config, shutdown.clone()).await
        })?
        .code()
        .unwrap_or(1)
        .try_into()
        .unwrap_or(1);

    Ok(ExitCode::from(status))
}

async fn restore_if_needed(backup: &RestoreConfig, shutdown: Shutdown) -> anyhow::Result<()> {
    if backup.dst.exists() && backup.strategy != Some(config::RestoreStrategy::Always) {
        logPrint!(
            "supervisor",
            "Directory {} exists, skipping restore",
            backup.dst.display()
        );
        return Ok(());
    }

    let mut process =
        Process::new("restore", restore(backup), shutdown).context("Starting restoring process")?;

    process
        .wait()
        .await
        .context("Waiting for restoring process")?;

    Ok(())
}

async fn sleep_until_or_forever(until: Option<Instant>) {
    match until {
        Some(until) => sleep_until(until.into()).await,
        None => pending().await,
    }
}

async fn monitor_ctrl_c(shutdown: Shutdown) {
    let _ = ctrl_c().await;
    logPrint!("supervisor", "Received Ctrl-C, shutting down");
    shutdown.shutdown();
}

async fn start_backup(
    backup: &BackupConfig,
    app: &AppConfig,
    shutdown: Shutdown,
    mut app_process: Process,
) -> anyhow::Result<Process> {
    let stopping_app = backup.strategy.unwrap_or_default() == config::BackupStrategy::StopApp;

    if stopping_app {
        logPrint!("supervisor", "Stopping app before starting backup");
        let _ = app_process.terminate_and_wait().await;
    }

    let mut process = Process::new("backup", backup::backup(backup), shutdown.clone())
        .context("Starting backup process")?;

    if !process
        .wait()
        .await
        .context("Waiting for backup process")?
        .success()
    {
        bail!("Failed backing up app");
    }

    if stopping_app {
        logPrint!("supervisor", "Starting app after backup");
        app_process =
            Process::new("app", runner::run_app(app), shutdown).context("Starting app process")?;
    }

    Ok(app_process)
}

async fn start_update(
    app: &AppConfig,
    mut app_process: Process,
    shutdown: Shutdown,
) -> anyhow::Result<Process> {
    let old_time = image_info::image_creation_time(&app.image)
        .await
        .context("Getting image creation time")?;

    logPrint!("supervisor", "Pulling latest image for {}", app.image);

    let mut process = Process::new("update", pull_image(&app), shutdown.clone())
        .context("Starting update process")?;
    process.wait().await.context("Waiting for update process")?;

    let new_time = image_info::image_creation_time(&app.image)
        .await
        .context("Getting image creation time")?;

    if new_time.is_some() && new_time != old_time {
        logPrint!("supervisor", "Image updated, restarting app");
        app_process
            .terminate_and_wait()
            .await
            .context("Terminating app")?;

        app_process =
            Process::new("app", runner::run_app(app), shutdown).context("Starting app process")?;
    } else {
        logPrint!("supervisor", "Image not updated. Do nothing");
    }

    Ok(app_process)
}

async fn run(config: config::Config, shutdown: Shutdown) -> anyhow::Result<ExitStatus> {
    let config::Config {
        backup,
        app,
        update,
        restore,
    } = config;
    let update = update.unwrap_or_default();

    if let Some(restore) = &restore {
        restore_if_needed(restore, shutdown.clone()).await?;
        if shutdown.shutdown_started() {
            bail!("Shutting down while restoring backup")
        }
    }

    let tz = current_timezone();

    let mut last_update = None;
    let mut last_backup = None;

    if let Some(backup) = &backup {
        last_backup = restic::get_latest_snapshot_time(backup)
            .await
            .map(|s| s.with_timezone(&tz));
    }

    let mut process = Process::new("app", runner::run_app(&app), shutdown.clone())
        .context("Starting app process")?;

    while !shutdown.shutdown_started() {
        let now = Utc::now().with_timezone(&tz);

        let next_backup = backup
            .as_ref()
            .and_then(|b| b.interval.next(last_backup, now))
            .map(|d| {
                logPrint!("supervisor", "Next backup time is in {d:?}");
                Instant::now() + d
            });

        let next_update = update.interval.next(last_update, now).map(|d| {
            logPrint!("supervisor", "Next update time is in {d:?}");
            Instant::now() + d
        });

        select! {
            _ = sleep_until_or_forever(next_backup) => {
                let backup = backup.as_ref().unwrap();
                process = start_backup(backup, &app, shutdown.clone(), process).await.context("Running backup process")?;
                last_backup = Some(Utc::now().with_timezone(&tz));
            }

            _ = sleep_until_or_forever(next_update) => {
                process = start_update(&app, process, shutdown.clone()).await.context("Running update process")?;
                last_update = Some(Utc::now().with_timezone(&tz));
            }

            status = process.wait() => {
                return status
            }
        }
    }

    bail!("Shutting down")
}
