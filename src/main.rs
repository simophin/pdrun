mod config;
mod runner;

use std::{io::BufReader, path::PathBuf, process::ExitCode};

use anyhow::Context;
use clap::Parser;
use tokio::runtime::Runtime;

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

    rt.block_on(run(&rt, config))
}

async fn run(rt: &Runtime, config: config::Config) -> anyhow::Result<ExitCode> {
    let handle = runner::run_app(rt, config.app).context("Start running app")?;

    Ok(handle.await.context("Waiting for app to exit")??)
}
