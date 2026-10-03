use crate::config::PluginConfig;
use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::{
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
const STDOUT_LIMIT: u64 = 8 * 1024 * 1024;
const STDERR_LIMIT: u64 = 64 * 1024;
const INPUT_LIMIT: usize = 64 * 1024 * 1024;
pub(crate) fn run<T: Serialize, R>(
    request: &T,
    command_config: &PluginConfig,
    cwd: &Path,
    validate: impl FnOnce(&[u8]) -> Result<R>,
) -> Result<R> {
    #[cfg(not(unix))]
    {
        let _ = (request, command_config, cwd, validate);
        bail!("subprocess execution requires Unix process-group cleanup in this release");
    }
    #[cfg(unix)]
    {
        run_unix(request, command_config, cwd, validate)
    }
}
#[cfg(unix)]
fn run_unix<T: Serialize, R>(
    request: &T,
    plugin: &PluginConfig,
    cwd: &Path,
    validate: impl FnOnce(&[u8]) -> Result<R>,
) -> Result<R> {
    use nix::{
        sys::signal::{Signal, killpg},
        unistd::Pid,
    };
    use std::os::unix::process::CommandExt;
    let started = Instant::now();
    let deadline = started
        .checked_add(Duration::from_millis(plugin.timeout_ms))
        .context("plugin timeout exceeds clock range")?;
    let request = serde_json::to_vec(request)?;
    if request.len() > INPUT_LIMIT {
        bail!("input exceeds {INPUT_LIMIT} bytes");
    }
    if Instant::now() >= deadline {
        bail!("deadline exceeded during input serialization");
    }
    let executable = plugin.command.first().context("missing plugin command")?;
    let mut command = Command::new(executable);
    command
        .args(&plugin.command[1..])
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = command.spawn().context("starting executable")?;
    let group = Pid::from_raw(child.id().try_into().context("child process ID")?);
    let mut stdin = child.stdin.take().context("stdin unavailable")?;
    let stdout = child.stdout.take().context("stdout unavailable")?;
    let stderr = child.stderr.take().context("stderr unavailable")?;
    let (input_tx, input_rx) = mpsc::channel();
    thread::spawn(move || {
        let result = stdin
            .write_all(&request)
            .and_then(|()| stdin.write_all(b"\n"));
        drop(stdin);
        let _ = input_tx.send(result);
    });
    let output_rx = read_pipe(stdout, STDOUT_LIMIT);
    let error_rx = read_pipe(stderr, STDERR_LIMIT);
    let result = (|| {
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if Instant::now() >= deadline {
                bail!("deadline exceeded after {} ms", plugin.timeout_ms);
            }
            thread::sleep(Duration::from_millis(5));
        };
        // Descendants must not keep protocol pipes open after the executable exits.
        let _ = killpg(group, Signal::SIGKILL);
        let remaining = || deadline.saturating_duration_since(Instant::now());
        let output = output_rx
            .recv_timeout(remaining())
            .context("stdout did not close within deadline")??;
        let error = error_rx
            .recv_timeout(remaining())
            .context("stderr did not close within deadline")??;
        if output.len() as u64 > STDOUT_LIMIT {
            bail!("stdout exceeds {STDOUT_LIMIT} bytes");
        }
        if error.len() as u64 > STDERR_LIMIT {
            bail!("stderr exceeds {STDERR_LIMIT} bytes");
        }
        if !status.success() {
            bail!(
                "executable exited {status}: {}",
                String::from_utf8_lossy(&error)
            );
        }
        input_rx
            .recv_timeout(remaining())
            .context("stdin delivery deadline")??;
        let response = validate(&output)?;
        if Instant::now() >= deadline {
            bail!("deadline exceeded during response validation");
        }
        Ok(response)
    })();
    let _ = killpg(group, Signal::SIGKILL);
    if result.is_err() {
        let _ = child.kill();
    }
    let _ = child.wait();
    result
}
fn read_pipe<R: Read + Send + 'static>(
    pipe: R,
    limit: u64,
) -> mpsc::Receiver<std::io::Result<Vec<u8>>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = vec![];
        let result = pipe.take(limit + 1).read_to_end(&mut bytes).map(|_| bytes);
        let _ = tx.send(result);
    });
    rx
}
