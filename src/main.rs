mod backup;
mod config;
mod image_info;
mod process;
mod runner;

use std::{
    io::BufReader,
    path::PathBuf,
    process::{ExitCode, ExitStatus},
};

use anyhow::{bail, Context};
use async_shutdown::Shutdown;
use chrono::{Duration, Utc};
use clap::Parser;
use config::{AppConfig, BackupConfig, UpdateConfig};
use futures::future::join_all;
use tokio::{
    select,
    signal::ctrl_c,
    task::{spawn_local, LocalSet},
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

async fn restore(backups: &Vec<BackupConfig>, shutdown: Shutdown) -> anyhow::Result<()> {
    if backups.is_empty() {
        log::info!(target: "supervisor", "No backup config found, skipping restoring.");
    }

    // Restore backup if the backup directory is
    let mut restore_handles = vec![];
    for (i, backup) in backups.iter().enumerate() {
        if backup.src.exists() {
            log::info!(
                target: "supervisor",
                "Backup directory {} exists, skipping restore",
                backup.src.display()
            );
            continue;
        }

        let mut process = Process::new(
            format!("restore_{i}"),
            backup::restore(backup),
            shutdown.clone(),
        )
        .context("Starting restoring process")?;

        restore_handles.push(spawn_local(
            async move { process.terminate_and_wait().await },
        ));
    }

    for status in join_all(restore_handles).await {
        if !status
            .context("Waiting for status")?
            .context("Restoring process")?
            .success()
        {
            bail!("Restoring process failed");
        }
    }

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

    Ok((now + update.interval).signed_duration_since(now))
}

async fn monitor_ctrl_c(shutdown: Shutdown) {
    let _ = ctrl_c().await;
    log::info!(target: "supervisor", "Received Ctrl-C, shutting down");
    shutdown.shutdown();
}

async fn run(config: config::Config, shutdown: Shutdown) -> anyhow::Result<ExitStatus> {
    let config::Config {
        backups,
        app,
        update,
    } = config;
    let backups = backups.unwrap_or_default();
    let update = update.unwrap_or_default();

    restore(&backups, shutdown.clone()).await?;

    let mut next_update = init_pull_image(&app, &update, shutdown.clone()).await?;
    log::info!(target: "supervisor", "Next image update time is: {next_update}");

    let mut process =
        Process::new("app", runner::run_app(&app), shutdown).context("Starting app process")?;

    loop {
        select! {}
    }

    process.wait().await.context("Waiting for app process")
}
