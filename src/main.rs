mod backup;
mod config;
mod process;
mod runner;

use std::{
    io::BufReader,
    path::PathBuf,
    process::{ExitCode, ExitStatus},
};

use anyhow::Context;
use clap::Parser;
use tokio::task::LocalSet;

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
        .block_on(&rt, run(config))?
        .code()
        .unwrap_or(1)
        .try_into()
        .unwrap_or(1);

    Ok(ExitCode::from(status))
}

async fn run(config: config::Config) -> anyhow::Result<ExitStatus> {
    // let mut child = runner::run_app(&config.app).context("Starting container")?;
    // loop {
    //     let mut status = select! {
    //         _ = tokio::signal::ctrl_c() => {
    //             log::info!("Ctrl-C received, stopping container");
    //             child.kill().await?;
    //             None
    //         }

    //         status = child.wait() => {
    //             Some(status.context("Waiting for container status")?)
    //         }

    //     };

    //     if status.is_none() {
    //         status = child.wait().await.context("Waiting for container status")?;
    //     }
    // }

    todo!()
}
