use std::process::Stdio;

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use tokio::{io::AsyncReadExt, process::Command};

#[derive(Deserialize)]
struct ImageInfo {
    #[serde(rename = "Created")]
    created: DateTime<Utc>,
}

pub async fn image_creation_time(image_name: &str) -> anyhow::Result<Option<DateTime<Utc>>> {
    let mut cmd = Command::new("podman");
    cmd.arg("inspect")
        .arg(image_name)
        .args(["--format", "json"])
        .kill_on_drop(true)
        .stdout(Stdio::piped());

    let mut child = cmd.spawn().expect("to spawn a child process");

    let status = child.wait().await.context("Waiting for child process")?;

    if !status.success() {
        return Ok(None);
    }

    let mut stdout = child.stdout.take().context("To take stdout from process")?;
    let mut json = Default::default();

    stdout
        .read_to_string(&mut json)
        .await
        .context("Reading output")?;

    let results: Vec<ImageInfo> = serde_json::from_str(&json).context("Deserialize image info")?;

    Ok(results.into_iter().next().map(|s| s.created))
}
