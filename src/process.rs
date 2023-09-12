use std::{
    process::{ExitStatus, Stdio},
    time::Duration,
};

use anyhow::{anyhow, Context};
use async_shutdown::Shutdown;
use nix::{
    sys::signal::{kill, Signal::SIGTERM},
    unistd::Pid,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    process::{Child, Command},
    sync::watch,
    task::spawn_local,
    time::timeout,
};

use crate::log::{elogPrint, logPrint};

pub struct Process {
    shutdown: Shutdown,
    exit_watcher: watch::Receiver<Option<anyhow::Result<ExitStatus>>>,
}

impl Process {
    pub fn new(
        log_prefix: impl AsRef<str>,
        mut child: Command,
        shutdown: Shutdown,
    ) -> anyhow::Result<Self> {
        logPrint!(
            "supervisor",
            "Starting child process: {}",
            log_prefix.as_ref()
        );

        let mut child = child
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("Start running child")?;

        let stdout = child
            .stdout
            .take()
            .context("Expecting stdout from child process")?;

        let stderr = child
            .stderr
            .take()
            .context("Expecting stderr from child process")?;

        let child_pid = Pid::from_raw(child.id().context("To have a PID")? as i32);

        let log_prefix = format!(
            "{log_prefix}({child_pid})",
            log_prefix = log_prefix.as_ref(),
            child_pid = child_pid.as_raw()
        );

        let (exit_sender, exit_watcher) = watch::channel(None);

        spawn_local(redirect_output(log_prefix.clone(), stdout));
        spawn_local(redirect_error(log_prefix.clone(), stderr));

        {
            let shutdown = shutdown.clone();
            spawn_local(async move {
                let status =
                    monitor_exit_status(child, child_pid, log_prefix.clone(), shutdown).await;

                match &status {
                    Ok(status) if status.success() => {
                        logPrint!(
                            "supervisor",
                            "Child process {log_prefix} exited successfully"
                        );
                    }

                    Ok(status) => {
                        logPrint!(
                            "supervisor",
                            "Child process {log_prefix} exited with status {status}"
                        );
                    }

                    Err(err) => {
                        elogPrint!(
                            "supervisor",
                            "Getting exit code for process {log_prefix} encountered error: {err:?}"
                        );
                    }
                }

                let _ = exit_sender.send_replace(Some(status));
            })
        };

        Ok(Self {
            shutdown,
            exit_watcher,
        })
    }

    pub async fn wait(&mut self) -> anyhow::Result<ExitStatus> {
        self.exit_watcher
            .wait_for(|s| s.is_some())
            .await
            .context("Waiting for status")?;

        match self.exit_watcher.borrow().as_ref() {
            Some(Ok(status)) => Ok(status.clone()),
            Some(Err(err)) => Err(anyhow!("Child process exited with error: {err:?}")),
            None => panic!("Must have result"),
        }
    }

    pub async fn terminate_and_wait(&mut self) -> anyhow::Result<ExitStatus> {
        self.shutdown.shutdown();
        self.wait().await
    }
}

async fn monitor_exit_status(
    mut child: Child,
    child_pid: Pid,
    log_prefix: String,
    shutdown: Shutdown,
) -> anyhow::Result<ExitStatus> {
    match shutdown.wrap_cancel(child.wait()).await {
        Some(status) => return status.context("Getting exit status"),
        None => {
            logPrint!("supervisor", "Terminating child process {log_prefix}");
            kill(child_pid, SIGTERM).with_context(|| format!("Sending SIGTERM to {log_prefix}"))?;
        }
    }

    match timeout(Duration::from_secs(5), child.wait()).await {
        Ok(status) => return status.context("Getting exit status"),
        Err(_) => {
            logPrint!(
                "supervisor",
                "Child process {log_prefix} did not terminate in 5 seconds, killing it"
            );

            child.start_kill().context("Start killing child process")?;
            child
                .wait()
                .await
                .with_context(|| format!("Waiting for {log_prefix} to exit"))
        }
    }
}

async fn redirect_output(log_prefix: String, from: impl AsyncRead + Unpin) -> anyhow::Result<()> {
    let mut from = BufReader::new(from);
    let mut line = String::default();
    while from.read_line(&mut line).await.context("Read line")? > 0 {
        logPrint!(&log_prefix, "{}", line.trim_end());
        line.clear();
    }

    Ok(())
}

async fn redirect_error(log_prefix: String, from: impl AsyncRead + Unpin) -> anyhow::Result<()> {
    let mut from = BufReader::new(from);
    let mut line = String::default();
    while from.read_line(&mut line).await.context("Read line")? > 0 {
        elogPrint!(&log_prefix, "{}", line.trim_end());
        line.clear();
    }

    Ok(())
}
