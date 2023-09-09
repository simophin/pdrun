use std::{io::Write, process::ExitStatus, sync::Arc};

use anyhow::Context;
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    process::{Child, ChildStderr, ChildStdout},
    select,
    sync::mpsc,
    sync::watch,
    task::spawn_local,
};

pub struct Process {
    command_tx: mpsc::Sender<Command>,
    exit_watcher: watch::Receiver<Option<anyhow::Result<ExitStatus>>>,
}

enum Command {
    Kill,
}

impl Process {
    pub fn new(log_prefix: String, mut child: Child) -> anyhow::Result<Self> {
        let stdout = child
            .stdout
            .take()
            .context("Expecting stdout from child process")?;

        let stderr = child
            .stderr
            .take()
            .context("Expecting stderr from child process")?;

        let (command_tx, command_rx) = mpsc::channel(1);
        let (exit_sender, exit_watcher) = watch::channel(None);

        spawn_local(monitor(
            child,
            log_prefix,
            stdout,
            stderr,
            command_rx,
            exit_sender,
        ));

        Ok(Self {
            command_tx,
            exit_watcher,
        })
    }

    async fn kill(&mut self) -> anyhow::Result<()> {
        self.command_tx.send(Command::Kill).await?;
        Ok(())
    }

    async fn wait(&mut self) -> anyhow::Result<ExitStatus> {
        if let Some(s) = self.exit_watcher.borrow().as_ref() {
            return s.clone();
        }
        todo!()
    }
}

async fn monitor(
    mut child: Child,
    log_prefix: String,
    stdout: ChildStdout,
    stderr: ChildStderr,
    mut command_rx: mpsc::Receiver<Command>,
    exit_sender: watch::Sender<Option<anyhow::Result<ExitStatus>>>,
) {
    spawn_local(redirect_output(
        log_prefix.clone(),
        stdout,
        std::io::stdout(),
    ));

    spawn_local(redirect_output(
        log_prefix.clone(),
        stderr,
        std::io::stderr(),
    ));

    let status = select! {
        _ = command_rx.recv() => {
            println!("[supervisor] Receive kill command, terminating child process");
            let _ = child.kill().await;
            child.wait().await
        }

        status = child.wait() => {
            println!("[supervisor] Child process exited with {status:?}");
            status
        }
    };

    let _ = exit_sender.send_replace(Some(
        status.context("Waiting for child process exit status"),
    ));
}

async fn redirect_output(log_prefix: String, from: impl AsyncRead + Unpin, mut to: impl Write) {
    let mut from = BufReader::new(from);
    let mut line = String::default();
    while from.read_line(&mut line).await.is_ok() {
        let _ = writeln!(to, "{log_prefix}{line}");
        line.clear();
    }
}
