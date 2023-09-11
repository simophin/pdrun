use tokio::process::Command;

use super::config::AppConfig;

pub fn pull_image(config: &AppConfig) -> Command {
    let mut cmd = Command::new("podman");
    cmd.arg("pull").arg(&config.image);
    cmd
}

pub fn run_app(config: &AppConfig) -> Command {
    let mut cmd = Command::new("podman");

    cmd.arg("run");

    let AppConfig {
        image,
        args,
        volumes,
        cap_add,
        environments,
        ports,
        network_mode,
    } = config;

    if let Some(envs) = &environments {
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

    if let Some(ports) = ports {
        for port in ports {
            cmd.arg("-p").arg(port);
        }
    }

    if let Some(network_mode) = network_mode {
        cmd.arg("--network").arg(network_mode.to_string());
    }

    cmd.args(["--rm", "--init"]);
    cmd.arg(image);

    if let Some(args) = args {
        cmd.args(args);
    }

    cmd
}
