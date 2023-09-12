mod backup;
mod config;
mod image_info;

mod log;
mod process;
mod restic;
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
use runner::pull_image;
use tokio::{
    select,
    signal::ctrl_c,
    task::{spawn_local, LocalSet},
    time::sleep_until,
};

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
        logPrint!(
            "supervisor",
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
            logPrint!("supervisor", "Skipped updating image");
        }

        _ => {
            logPrint!("supervisor", "Updating image");
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

    logPrint!("supervisor", "Starting backup process");

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

    if new_time != old_time {
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
    } = config;
    let update = update.unwrap_or_default();

    if let Some(backup) = &backup {
        restore(backup, shutdown.clone()).await?;
    }

    let mut next_update = Instant::now() + init_pull_image(&app, &update, shutdown.clone()).await?;
    logPrint!("supervisor", "Next image update time is: {next_update:?}");

    let mut process = Process::new("app", runner::run_app(&app), shutdown.clone())
        .context("Starting app process")?;

    let mut last_backup;
    let mut next_backup;

    if let Some(backup) = &backup {
        last_backup = restic::get_latest_snapshot_time(backup)
            .await
            .context("Getting latest backup time")?;
        next_backup = Some(Instant::now() + next_backup_time(backup, last_backup));
    } else {
        last_backup = None;
        next_backup = None;
    }

    loop {
        select! {
            _ = sleep_until_or_forever(next_backup) => {
                let backup = backup.as_ref().unwrap();
                process = start_backup(backup, &app, shutdown.clone(), process).await.context("Running backup process")?;
                last_backup = Some(Utc::now());
                let duration = next_backup_time(backup, last_backup);
                logPrint!("supervisor", "Next backup time is in {duration:?}");
                next_backup = Some(Instant::now() + duration);
            }

            _ = sleep_until(next_update.into()) => {
                process = start_update(&app, process, shutdown.clone()).await.context("Running update process")?;
                let duration = update.interval.to_duration(Utc::now());
                logPrint!("supervisor", "Next image update time is in {duration:?}");
                next_update = Instant::now() + duration;
            }

            status = process.wait() => {
                return status
            }
        }
    }
}
