use crate::config::PluginConfig;
use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::{
    path::Path,
    process::{Command, ExitStatus},
    time::{Duration, Instant},
};

const STDOUT_LIMIT: usize = 8 * 1024 * 1024;
const STDERR_LIMIT: usize = 64 * 1024;
const INPUT_LIMIT: usize = 64 * 1024 * 1024;
const CLEANUP_TIME: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Default)]
pub(crate) struct ChildLimits {
    pub core_bytes: Option<u64>,
    pub cpu_seconds: Option<u64>,
    pub file_bytes: Option<u64>,
    pub open_files: Option<u64>,
    pub address_space_bytes: Option<u64>,
}

pub(crate) struct CommandLimits {
    pub deadline: Instant,
    pub stdin_bytes: usize,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub resources: ChildLimits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CommandStop {
    Completed,
    TimedOut,
    Cancelled,
    OutputLimit(&'static str),
    Monitor(String),
    Io(String),
    Cleanup(String),
}

pub(crate) struct CommandOutput {
    pub status: Option<ExitStatus>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stop: CommandStop,
    pub stdin_complete: bool,
    pub cleanup_complete: bool,
}

pub(crate) fn run<T: Serialize, R>(
    request: &T,
    command_config: &PluginConfig,
    cwd: &Path,
    validate: impl FnOnce(&[u8]) -> Result<R>,
) -> Result<R> {
    let deadline = Instant::now()
        .checked_add(Duration::from_millis(command_config.timeout_ms))
        .context("plugin timeout exceeds clock range")?;
    let mut request = serde_json::to_vec(request)?;
    if request.len() > INPUT_LIMIT {
        bail!("input exceeds {INPUT_LIMIT} bytes");
    }
    if Instant::now() >= deadline {
        bail!("deadline exceeded during input serialization");
    }
    request.push(b'\n');
    let executable = command_config
        .command
        .first()
        .context("missing plugin command")?;
    let mut command = Command::new(executable);
    command.args(&command_config.command[1..]).current_dir(cwd);
    let output = run_command(
        &mut command,
        &request,
        &CommandLimits {
            deadline,
            stdin_bytes: INPUT_LIMIT + 1,
            stdout_bytes: STDOUT_LIMIT,
            stderr_bytes: STDERR_LIMIT,
            resources: ChildLimits::default(),
        },
        || false,
        || Ok(()),
    )?;
    match output.stop {
        CommandStop::Completed => {}
        CommandStop::TimedOut => bail!("deadline exceeded after {} ms", command_config.timeout_ms),
        CommandStop::Cancelled => bail!("execution cancelled"),
        CommandStop::OutputLimit(stream) => bail!("{stream} exceeds its output limit"),
        CommandStop::Monitor(message)
        | CommandStop::Io(message)
        | CommandStop::Cleanup(message) => {
            bail!("{message}")
        }
    }
    if !output.cleanup_complete {
        bail!("process cleanup incomplete");
    }
    let status = output.status.context("executable status unavailable")?;
    if !status.success() {
        bail!(
            "executable exited {status}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    if !output.stdin_complete {
        bail!("stdin delivery incomplete");
    }
    let response = validate(&output.stdout)?;
    if Instant::now() >= deadline {
        bail!("deadline exceeded during response validation");
    }
    Ok(response)
}

pub(crate) fn run_command(
    command: &mut Command,
    input: &[u8],
    limits: &CommandLimits,
    cancelled: impl Fn() -> bool,
    monitor: impl FnMut() -> Result<()>,
) -> Result<CommandOutput> {
    if input.len() > limits.stdin_bytes {
        bail!("input exceeds {} bytes", limits.stdin_bytes);
    }
    #[cfg(not(unix))]
    {
        let _ = (command, input, limits, cancelled, monitor);
        bail!("subprocess execution requires Unix process-group cleanup in this release");
    }
    #[cfg(unix)]
    unix::run(command, input, limits, cancelled, monitor)
}

#[cfg(unix)]
mod unix {
    use super::*;
    use nix::{
        errno::Errno,
        fcntl::{FcntlArg, OFlag, fcntl},
        poll::{PollFd, PollFlags, PollTimeout, poll},
        sys::{
            resource::{Resource, setrlimit},
            signal::{Signal, killpg},
        },
        unistd::Pid,
    };
    use std::{
        io::{self, Read, Write},
        os::{fd::AsFd, unix::process::CommandExt},
        process::{Child, Stdio},
        thread,
    };

    struct ChildGuard {
        child: Child,
        group: Pid,
        group_cleaned: bool,
        cleaned: bool,
    }

    impl ChildGuard {
        fn kill_group(&mut self) -> Result<()> {
            if self.group_cleaned {
                return Ok(());
            }
            match killpg(self.group, Signal::SIGKILL) {
                Ok(()) | Err(Errno::ESRCH) => {
                    self.group_cleaned = true;
                    Ok(())
                }
                Err(error) => Err(error).context("process-group cleanup"),
            }
        }

        #[cfg(any(target_os = "linux", target_os = "macos"))]
        fn exited(&mut self) -> Result<bool> {
            // WNOWAIT reserves the leader PID until its group has been killed.
            let mut info: nix::libc::siginfo_t = unsafe { std::mem::zeroed() };
            let result = unsafe {
                nix::libc::waitid(
                    nix::libc::P_PID,
                    self.group.as_raw() as nix::libc::id_t,
                    &mut info,
                    nix::libc::WEXITED | nix::libc::WNOHANG | nix::libc::WNOWAIT,
                )
            };
            if result == -1 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::Interrupted {
                    return Ok(false);
                }
                return Err(error).context("observing executable exit");
            }
            Ok(unsafe { info.si_pid() } == self.group.as_raw())
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        fn exited(&mut self) -> Result<bool> {
            Ok(self.child.try_wait()?.is_some())
        }

        fn finish(&mut self) -> Result<ExitStatus> {
            let group_result = self.kill_group();
            let _ = self.child.kill();
            let deadline = Instant::now() + CLEANUP_TIME;
            loop {
                if let Some(status) = self.child.try_wait().context("reaping executable")? {
                    self.cleaned = true;
                    group_result?;
                    return Ok(status);
                }
                if Instant::now() >= deadline {
                    bail!("executable did not exit within cleanup deadline");
                }
                thread::sleep(Duration::from_millis(2));
            }
        }
    }

    impl Drop for ChildGuard {
        fn drop(&mut self) {
            if !self.cleaned {
                let _ = self.finish();
            }
        }
    }

    fn nonblocking(fd: &impl AsFd) -> Result<()> {
        let flags = fcntl(fd, FcntlArg::F_GETFL)?;
        fcntl(
            fd,
            FcntlArg::F_SETFL(OFlag::from_bits_retain(flags) | OFlag::O_NONBLOCK),
        )?;
        Ok(())
    }

    fn resources(limits: &ChildLimits) -> io::Result<()> {
        let settings = [
            (Resource::RLIMIT_CORE, limits.core_bytes),
            (Resource::RLIMIT_CPU, limits.cpu_seconds),
            (Resource::RLIMIT_FSIZE, limits.file_bytes),
            (Resource::RLIMIT_NOFILE, limits.open_files),
            (Resource::RLIMIT_AS, limits.address_space_bytes),
        ];
        for (resource, value) in settings {
            if let Some(value) = value {
                setrlimit(resource, value, value).map_err(io::Error::from)?;
            }
        }
        Ok(())
    }

    pub(super) fn run(
        command: &mut Command,
        input: &[u8],
        limits: &CommandLimits,
        cancelled: impl Fn() -> bool,
        mut monitor: impl FnMut() -> Result<()>,
    ) -> Result<CommandOutput> {
        if cancelled() || Instant::now() >= limits.deadline {
            return Ok(CommandOutput {
                status: None,
                stdout: vec![],
                stderr: vec![],
                stop: if cancelled() {
                    CommandStop::Cancelled
                } else {
                    CommandStop::TimedOut
                },
                stdin_complete: input.is_empty(),
                cleanup_complete: true,
            });
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        let resource_limits = limits.resources.clone();
        // Only direct setrlimit calls are permitted between fork and exec.
        unsafe {
            command.pre_exec(move || resources(&resource_limits));
        }
        let mut child = command.spawn().context("starting executable")?;
        let group = match i32::try_from(child.id()) {
            Ok(id) => Pid::from_raw(id),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error).context("child process ID");
            }
        };
        let mut guard = ChildGuard {
            child,
            group,
            group_cleaned: false,
            cleaned: false,
        };
        let stdin = guard.child.stdin.take().context("stdin unavailable")?;
        let stdout = guard.child.stdout.take().context("stdout unavailable")?;
        let stderr = guard.child.stderr.take().context("stderr unavailable")?;
        nonblocking(&stdin)?;
        nonblocking(&stdout)?;
        nonblocking(&stderr)?;
        let mut stdin = if input.is_empty() { None } else { Some(stdin) };
        let mut stdout = Some(stdout);
        let mut stderr = Some(stderr);
        let mut output = CommandOutput {
            status: None,
            stdout: vec![],
            stderr: vec![],
            stop: CommandStop::Completed,
            stdin_complete: input.is_empty(),
            cleanup_complete: false,
        };
        let mut input_at = 0usize;
        let mut drain_deadline = None;
        let mut next_monitor = Instant::now();
        loop {
            if cancelled() {
                output.stop = CommandStop::Cancelled;
                break;
            }
            let now = Instant::now();
            if now >= limits.deadline {
                output.stop = CommandStop::TimedOut;
                break;
            }
            if drain_deadline.is_some_and(|deadline| now >= deadline) {
                output.stop = CommandStop::Cleanup(
                    "stdout or stderr stayed open after executable exit".into(),
                );
                break;
            }
            if now >= next_monitor {
                if let Err(error) = monitor() {
                    output.stop = CommandStop::Monitor(format!("{error:#}"));
                    break;
                }
                next_monitor = now + Duration::from_millis(25);
            }
            if let Err(stop) = drain(
                &mut stdout,
                &mut output.stdout,
                limits.stdout_bytes,
                "stdout",
            ) {
                output.stop = stop;
                break;
            }
            if let Err(stop) = drain(
                &mut stderr,
                &mut output.stderr,
                limits.stderr_bytes,
                "stderr",
            ) {
                output.stop = stop;
                break;
            }
            if let Some(pipe) = &mut stdin {
                let end = input.len().min(input_at.saturating_add(65536));
                match pipe.write(&input[input_at..end]) {
                    Ok(0) => {
                        output.stop = CommandStop::Io("stdin closed before delivery".into());
                        break;
                    }
                    Ok(count) => {
                        input_at += count;
                        if input_at == input.len() {
                            output.stdin_complete = true;
                            stdin = None;
                        }
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                        ) => {}
                    Err(error) => {
                        output.stop = CommandStop::Io(format!("stdin delivery: {error}"));
                        break;
                    }
                }
            }
            if output.status.is_none() {
                match guard.exited() {
                    Ok(true) => {
                        stdin = None;
                        if let Err(error) = guard.kill_group() {
                            output.stop = CommandStop::Cleanup(format!("{error:#}"));
                            break;
                        }
                        match guard.child.try_wait() {
                            Ok(status) => output.status = status,
                            Err(error) => {
                                output.stop =
                                    CommandStop::Io(format!("reaping executable: {error}"));
                                break;
                            }
                        }
                        drain_deadline = Some(Instant::now() + CLEANUP_TIME);
                    }
                    Ok(false) => {}
                    Err(error) => {
                        output.stop = CommandStop::Io(format!("waiting for executable: {error}"));
                        break;
                    }
                }
            }
            if output.status.is_some() && stdout.is_none() && stderr.is_none() {
                break;
            }
            let mut pipes = vec![];
            if let Some(pipe) = &stdin {
                pipes.push(PollFd::new(pipe.as_fd(), PollFlags::POLLOUT));
            }
            if let Some(pipe) = &stdout {
                pipes.push(PollFd::new(pipe.as_fd(), PollFlags::POLLIN));
            }
            if let Some(pipe) = &stderr {
                pipes.push(PollFd::new(pipe.as_fd(), PollFlags::POLLIN));
            }
            match poll(&mut pipes, PollTimeout::from(5u8)) {
                Ok(_) | Err(Errno::EINTR) => {}
                Err(error) => {
                    output.stop = CommandStop::Io(format!("polling command pipes: {error}"));
                    break;
                }
            }
        }
        drop((stdin, stdout, stderr));
        match guard.finish() {
            Ok(status) => {
                output.status = Some(status);
                output.cleanup_complete = !matches!(output.stop, CommandStop::Cleanup(_));
            }
            Err(error) => {
                output.stop = CommandStop::Cleanup(format!("{error:#}"));
            }
        }
        Ok(output)
    }

    fn drain<R: Read>(
        pipe: &mut Option<R>,
        bytes: &mut Vec<u8>,
        limit: usize,
        name: &'static str,
    ) -> std::result::Result<(), CommandStop> {
        let Some(reader) = pipe.as_mut() else {
            return Ok(());
        };
        let mut buffer = [0u8; 8192];
        for _ in 0..16 {
            let available = limit.saturating_sub(bytes.len());
            let read_limit = available.saturating_add(1).min(buffer.len());
            match reader.read(&mut buffer[..read_limit]) {
                Ok(0) => {
                    *pipe = None;
                    return Ok(());
                }
                Ok(count) => {
                    bytes.extend_from_slice(&buffer[..count.min(available)]);
                    if count > available {
                        return Err(CommandStop::OutputLimit(name));
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(CommandStop::Io(format!("{name}: {error}"))),
            }
        }
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicBool, Ordering},
    };

    fn node(program: &str) -> Command {
        let mut command = Command::new("node");
        command.args(["-e", program]);
        command
    }

    fn limits() -> CommandLimits {
        CommandLimits {
            deadline: Instant::now() + Duration::from_secs(3),
            stdin_bytes: 2 * 1024 * 1024,
            stdout_bytes: 1024,
            stderr_bytes: 1024,
            resources: ChildLimits::default(),
        }
    }

    #[test]
    fn raw_transport_preserves_nonzero_status_and_both_streams() {
        let output = run_command(
            &mut node("process.stdout.write('out');process.stderr.write('err');process.exitCode=7"),
            b"",
            &limits(),
            || false,
            || Ok(()),
        )
        .unwrap();
        assert_eq!(output.stop, CommandStop::Completed);
        assert_eq!(output.status.unwrap().code(), Some(7));
        assert_eq!(output.stdout, b"out");
        assert_eq!(output.stderr, b"err");
        assert!(output.cleanup_complete);
    }

    #[test]
    fn raw_transport_closes_stdin_after_complete_delivery() {
        let output = run_command(
            &mut node("let text='';process.stdin.on('data',chunk=>text+=chunk);process.stdin.on('end',()=>process.stdout.write(text))"),
            b"request\n", &limits(), || false, || Ok(()),
        ).unwrap();
        assert_eq!(output.stop, CommandStop::Completed);
        assert_eq!(output.stdout, b"request\n");
        assert!(output.stdin_complete && output.cleanup_complete);
    }

    #[test]
    fn cancellation_before_spawn_has_no_filesystem_effects() {
        let root = tempfile::tempdir().unwrap();
        let marker = root.path().join("ran");
        let program = format!("require('node:fs').writeFileSync({marker:?},'ran')");
        let output = run_command(&mut node(&program), b"", &limits(), || true, || Ok(())).unwrap();
        assert_eq!(output.stop, CommandStop::Cancelled);
        assert!(output.status.is_none() && output.cleanup_complete);
        assert!(!marker.exists());
    }

    #[test]
    fn cancellation_after_spawn_reaps_the_command() {
        let flag = AtomicBool::new(false);
        let output = run_command(
            &mut node("setInterval(()=>{},1000)"),
            b"",
            &limits(),
            || flag.load(Ordering::Relaxed),
            || {
                flag.store(true, Ordering::Relaxed);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(output.stop, CommandStop::Cancelled);
        assert!(output.status.is_some() && output.cleanup_complete);
    }

    #[test]
    fn stream_limits_stop_commands_and_keep_only_bounded_bytes() {
        for (program, stream) in [
            (
                "process.stdout.write('x'.repeat(5000));setInterval(()=>{},1000)",
                "stdout",
            ),
            (
                "process.stderr.write('x'.repeat(5000));setInterval(()=>{},1000)",
                "stderr",
            ),
        ] {
            let output =
                run_command(&mut node(program), b"", &limits(), || false, || Ok(())).unwrap();
            assert_eq!(output.stop, CommandStop::OutputLimit(stream));
            assert!(output.stdout.len() <= 1024 && output.stderr.len() <= 1024);
            assert!(output.cleanup_complete);
        }
    }

    #[test]
    fn monitor_failure_stops_the_command_without_becoming_success() {
        let output = run_command(
            &mut node("setInterval(()=>{},1000)"),
            b"",
            &limits(),
            || false,
            || bail!("workspace budget control"),
        )
        .unwrap();
        assert_eq!(
            output.stop,
            CommandStop::Monitor("workspace budget control".into())
        );
        assert!(output.status.is_some() && output.cleanup_complete);
    }

    #[test]
    fn unread_stdin_deadline_does_not_leave_a_writer_thread() {
        let mut bound = limits();
        bound.deadline = Instant::now() + Duration::from_millis(250);
        let input = vec![b'x'; 2 * 1024 * 1024];
        let output = run_command(
            &mut node("setInterval(()=>{},1000)"),
            &input,
            &bound,
            || false,
            || Ok(()),
        )
        .unwrap();
        assert_eq!(output.stop, CommandStop::TimedOut);
        assert!(!output.stdin_complete && output.cleanup_complete);
    }

    #[test]
    fn escaped_descendant_pipes_produce_bounded_cleanup_failure() {
        use nix::{
            sys::signal::{Signal, kill},
            unistd::Pid,
        };
        let root = tempfile::tempdir().unwrap();
        let marker = root.path().join("pid");
        let program = format!(
            "const child=require('node:child_process').spawn(process.execPath,['-e','setInterval(()=>{{}},1000)'],{{detached:true,stdio:['ignore',1,2]}});require('node:fs').writeFileSync({marker:?},String(child.pid));child.unref();process.exit(0)"
        );
        let output = run_command(&mut node(&program), b"", &limits(), || false, || Ok(()));
        if let Ok(pid) = fs::read_to_string(&marker)
            .and_then(|text| text.parse::<i32>().map_err(std::io::Error::other))
        {
            let _ = kill(Pid::from_raw(pid), Signal::SIGKILL);
        }
        let output = output.unwrap();
        assert!(matches!(output.stop, CommandStop::Cleanup(_)));
        assert!(!output.cleanup_complete);
    }

    #[test]
    fn child_file_limit_restricts_generated_bytes() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("large");
        let mut command = Command::new("sh");
        command
            .args([
                "-c",
                "while :; do printf 0123456789; done > \"$ARCHGUARD_TEST_FILE\"",
            ])
            .env("ARCHGUARD_TEST_FILE", &file);
        let mut bound = limits();
        bound.resources.file_bytes = Some(4096);
        let output = run_command(&mut command, b"", &bound, || false, || Ok(())).unwrap();
        assert!(!output.status.unwrap().success());
        assert!(output.cleanup_complete);
        assert!(fs::metadata(file).unwrap().len() <= 4096);
    }
}
