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
        command,
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

// Callers must retain default SIGCHLD disposition and exclusively reap their own
// children for this call. Foreign waitpid calls can invalidate numeric cleanup.
pub(crate) fn run_command(
    command: Command,
    input: &[u8],
    limits: &CommandLimits,
    cancelled: impl Fn() -> bool,
    monitor: impl FnMut() -> Result<()>,
) -> Result<CommandOutput> {
    if input.len() > limits.stdin_bytes {
        bail!("input exceeds {} bytes", limits.stdin_bytes);
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (command, input, limits, cancelled, monitor);
        bail!("subprocess execution requires Linux or macOS process-group cleanup in this release");
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    unix::run(command, input, limits, cancelled, monitor)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
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
        owned: bool,
        finished: bool,
    }

    impl ChildGuard {
        fn kill_group(&mut self) -> Result<()> {
            self.exited()?;
            match killpg(self.group, Signal::SIGKILL) {
                Ok(()) | Err(Errno::ESRCH) => Ok(()),
                Err(error) => Err(error).context("process-group cleanup"),
            }
        }

        fn exited(&mut self) -> Result<bool> {
            if !self.owned {
                bail!("child ownership lost; numeric process cleanup suppressed");
            }
            // Reserve the leader PID until group liveness has been checked.
            for _ in 0..8 {
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
                    let error = io::Error::last_os_error();
                    if error.kind() == io::ErrorKind::Interrupted {
                        continue;
                    }
                    self.owned = false;
                    if error.raw_os_error() == Some(nix::libc::ECHILD) {
                        bail!("child ownership lost; numeric process cleanup suppressed");
                    }
                    return Err(error)
                        .context("child ownership observation failed; cleanup suppressed");
                }
                return Ok(unsafe { info.si_pid() } == self.group.as_raw());
            }
            self.owned = false;
            bail!("child ownership observation repeatedly interrupted; cleanup suppressed")
        }

        fn finish(&mut self) -> Result<ExitStatus> {
            self.finished = true;
            let mut failure = self.kill_group().err();
            if !self.owned {
                return Err(failure.unwrap_or_else(|| anyhow::anyhow!("child ownership lost")));
            }
            let _ = self.child.kill();
            let deadline = Instant::now() + CLEANUP_TIME;
            loop {
                let exited = self.exited()?;
                if exited {
                    match group_has_live_members(self.group) {
                        Ok(false) => {
                            let status = self
                                .child
                                .try_wait()
                                .context("reaping executable")?
                                .context("exited child status unavailable")?;
                            self.owned = false;
                            if let Some(error) = failure {
                                return Err(error);
                            }
                            return Ok(status);
                        }
                        Ok(true) => {}
                        Err(error) => {
                            let _ = self.child.try_wait();
                            self.owned = false;
                            return Err(error).context("group cleanup uncertain");
                        }
                    }
                }
                if Instant::now() >= deadline {
                    if exited {
                        let _ = self.child.try_wait();
                        self.owned = false;
                    }
                    bail!("process group still had live members at cleanup deadline");
                }
                if failure.is_none() {
                    failure = self.kill_group().err();
                }
                thread::sleep(Duration::from_millis(2));
            }
        }
    }

    impl Drop for ChildGuard {
        fn drop(&mut self) {
            if !self.finished && self.owned {
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

    fn require_child_ownership() -> Result<()> {
        let mut action: nix::libc::sigaction = unsafe { std::mem::zeroed() };
        let result =
            unsafe { nix::libc::sigaction(nix::libc::SIGCHLD, std::ptr::null(), &mut action) };
        if result == -1 {
            return Err(io::Error::last_os_error()).context("checking SIGCHLD disposition");
        }
        if action.sa_sigaction != nix::libc::SIG_DFL
            || action.sa_flags & nix::libc::SA_NOCLDWAIT != 0
        {
            bail!(
                "command execution requires default SIGCHLD disposition and exclusive child reaping"
            );
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn group_has_live_members(group: Pid) -> Result<bool> {
        use std::fs;
        for (index, entry) in fs::read_dir("/proc")
            .context("process-group inventory unavailable")?
            .enumerate()
        {
            if index >= 65536 {
                bail!("process inventory exceeds cleanup limit");
            }
            let entry = entry?;
            let name = entry.file_name();
            let Some(pid) = name.to_str().and_then(|text| text.parse::<i32>().ok()) else {
                continue;
            };
            match nix::unistd::getpgid(Some(Pid::from_raw(pid))) {
                Ok(current) if current == group => {}
                Ok(_) | Err(Errno::ESRCH) => continue,
                Err(error) => return Err(error).context("process-group membership observation"),
            }
            let file = match fs::File::open(entry.path().join("stat")) {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error).context("process-group member observation"),
            };
            let mut text = String::new();
            file.take(4097).read_to_string(&mut text)?;
            if text.len() > 4096 {
                bail!("process record exceeds cleanup limit");
            }
            let at = text.rfind(')').context("invalid process record")?;
            let fields: Vec<_> = text[at + 1..].split_ascii_whitespace().take(3).collect();
            if fields.len() != 3 {
                bail!("incomplete process record");
            }
            let pgrp: i32 = fields[2].parse().context("invalid process group")?;
            if pgrp == group.as_raw() && !matches!(fields[0], "Z" | "X") {
                return Ok(true);
            }
        }
        Ok(false)
    }

    #[cfg(target_os = "macos")]
    fn group_has_live_members(group: Pid) -> Result<bool> {
        const MAX_MEMBERS: usize = 4096;
        let mut pids = vec![0i32; MAX_MEMBERS];
        unsafe {
            *nix::libc::__error() = 0;
        }
        let count = unsafe {
            nix::libc::proc_listpgrppids(
                group.as_raw(),
                pids.as_mut_ptr().cast(),
                (pids.len() * size_of::<i32>()) as i32,
            )
        };
        if count < 0 || (count == 0 && unsafe { *nix::libc::__error() } != 0) {
            return Err(io::Error::last_os_error()).context("process-group inventory unavailable");
        }
        if count as usize >= MAX_MEMBERS {
            bail!("process group exceeds cleanup inventory limit");
        }
        for pid in pids.into_iter().take(count as usize).filter(|pid| *pid > 0) {
            let mut info: nix::libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
            let bytes = unsafe {
                nix::libc::proc_pidinfo(
                    pid,
                    nix::libc::PROC_PIDTBSDINFO,
                    0,
                    (&mut info as *mut nix::libc::proc_bsdinfo).cast(),
                    size_of::<nix::libc::proc_bsdinfo>() as i32,
                )
            };
            if bytes != size_of::<nix::libc::proc_bsdinfo>() as i32 {
                let error = io::Error::last_os_error();
                if error.raw_os_error() == Some(nix::libc::ESRCH) {
                    continue;
                }
                return Err(error).context("process-group member observation");
            }
            if info.pbi_pgid == group.as_raw() as u32 && info.pbi_status != nix::libc::SZOMB {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn resources(limits: &ChildLimits) -> io::Result<()> {
        let settings = [
            (Resource::RLIMIT_CORE, limits.core_bytes),
            (Resource::RLIMIT_CPU, limits.cpu_seconds),
            (Resource::RLIMIT_FSIZE, limits.file_bytes),
            (Resource::RLIMIT_NOFILE, limits.open_files),
        ];
        for (resource, value) in settings {
            if let Some(value) = value {
                setrlimit(resource, value, value).map_err(io::Error::from)?;
            }
        }
        if let Some(value) = limits.address_space_bytes {
            #[cfg(not(any(target_os = "freebsd", target_os = "netbsd", target_os = "openbsd")))]
            setrlimit(Resource::RLIMIT_AS, value, value).map_err(io::Error::from)?;
            #[cfg(any(target_os = "freebsd", target_os = "netbsd", target_os = "openbsd"))]
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "address-space limit unsupported",
            ));
        }
        Ok(())
    }

    pub(super) fn run(
        mut command: Command,
        input: &[u8],
        limits: &CommandLimits,
        cancelled: impl Fn() -> bool,
        mut monitor: impl FnMut() -> Result<()>,
    ) -> Result<CommandOutput> {
        require_child_ownership()?;
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
            owned: true,
            finished: false,
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
        let mut exited = false;
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
            if !exited {
                match guard.exited() {
                    Ok(true) => {
                        exited = true;
                        stdin = None;
                        if let Err(error) = guard.kill_group() {
                            output.stop = CommandStop::Cleanup(format!("{error:#}"));
                            break;
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
            if exited && stdout.is_none() && stderr.is_none() {
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

    #[cfg(test)]
    mod ownership_tests {
        use super::*;

        #[test]
        fn lost_child_ownership_suppresses_finish_and_drop_signals() {
            let mut command = Command::new("sh");
            command.args(["-c", "exit 0"]).process_group(0);
            let mut child = command.spawn().unwrap();
            let group = Pid::from_raw(child.id() as i32);
            child.wait().unwrap();
            let mut guard = ChildGuard {
                child,
                group,
                owned: true,
                finished: false,
            };
            assert!(
                guard
                    .finish()
                    .unwrap_err()
                    .to_string()
                    .contains("ownership lost")
            );
            assert!(!guard.owned && guard.finished);
            assert!(
                guard
                    .kill_group()
                    .unwrap_err()
                    .to_string()
                    .contains("cleanup suppressed")
            );
        }
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
            node("process.stdout.write('out');process.stderr.write('err');process.exitCode=7"),
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
            node("let text='';process.stdin.on('data',chunk=>text+=chunk);process.stdin.on('end',()=>process.stdout.write(text))"),
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
        let output = run_command(node(&program), b"", &limits(), || true, || Ok(())).unwrap();
        assert_eq!(output.stop, CommandStop::Cancelled);
        assert!(output.status.is_none() && output.cleanup_complete);
        assert!(!marker.exists());
    }

    #[test]
    fn cancellation_after_spawn_reaps_the_command() {
        let flag = AtomicBool::new(false);
        let output = run_command(
            node("setInterval(()=>{},1000)"),
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
            let output = run_command(node(program), b"", &limits(), || false, || Ok(())).unwrap();
            assert_eq!(output.stop, CommandStop::OutputLimit(stream));
            assert!(output.stdout.len() <= 1024 && output.stderr.len() <= 1024);
            assert!(output.cleanup_complete);
        }
    }

    #[test]
    fn monitor_failure_stops_the_command_without_becoming_success() {
        let output = run_command(
            node("setInterval(()=>{},1000)"),
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
            node("setInterval(()=>{},1000)"),
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
        let output = run_command(node(&program), b"", &limits(), || false, || Ok(()));
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
        let output = run_command(command, b"", &bound, || false, || Ok(())).unwrap();
        assert!(!output.status.unwrap().success());
        assert!(output.cleanup_complete);
        assert!(fs::metadata(file).unwrap().len() <= 4096);
    }
    #[test]
    fn independently_owned_commands_apply_current_resource_limits() {
        for files in [128u64, 256] {
            let mut command = Command::new("sh");
            command.args(["-c", "ulimit -n"]);
            let mut bound = limits();
            bound.resources.open_files = Some(files);
            let output = run_command(command, b"", &bound, || false, || Ok(())).unwrap();
            assert_eq!(output.stop, CommandStop::Completed);
            assert_eq!(
                String::from_utf8(output.stdout).unwrap().trim(),
                files.to_string()
            );
            assert!(output.cleanup_complete);
        }
    }

    #[test]
    fn auto_reap_dispositions_are_rejected_before_spawn() {
        use nix::sys::signal::{SaFlags, SigAction, SigHandler, SigSet, Signal, sigaction};
        const CASE: &str = "ARCHGUARD_SUBPROCESS_SIGCHLD_CONTROL";
        if let Ok(case) = std::env::var(CASE) {
            let action = if case == "ignore" {
                SigAction::new(SigHandler::SigIgn, SaFlags::empty(), SigSet::empty())
            } else {
                SigAction::new(SigHandler::SigDfl, SaFlags::SA_NOCLDWAIT, SigSet::empty())
            };
            unsafe {
                sigaction(Signal::SIGCHLD, &action).unwrap();
            }
            let root = tempfile::tempdir().unwrap();
            let marker = root.path().join("ran");
            let program = format!("require('node:fs').writeFileSync({marker:?},'ran')");
            let error = run_command(node(&program), b"", &limits(), || false, || Ok(()))
                .err()
                .unwrap();
            assert!(error.to_string().contains("SIGCHLD"));
            assert!(!marker.exists());
            return;
        }
        for case in ["ignore", "noChildWait"] {
            let mut command = Command::new(std::env::current_exe().unwrap());
            command.args([
                "subprocess::tests::auto_reap_dispositions_are_rejected_before_spawn",
                "--exact",
                "--nocapture",
            ]);
            command.env(CASE, case);
            let mut bound = limits();
            bound.stdout_bytes = 8192;
            bound.stderr_bytes = 8192;
            let output = run_command(command, b"", &bound, || false, || Ok(())).unwrap();
            assert_eq!(output.stop, CommandStop::Completed);
            assert!(output.status.unwrap().success() && output.cleanup_complete);
        }
    }
}
