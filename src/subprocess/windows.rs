use super::*;
use std::{
    cmp::Ordering,
    ffi::{OsStr, OsString},
    io,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
        process::ExitStatusExt,
    },
    ptr::{null, null_mut},
    thread,
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_BROKEN_PIPE, HANDLE, HANDLE_FLAG_INHERIT, SetHandleInformation,
        WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    Security::SECURITY_ATTRIBUTES,
    Storage::FileSystem::{
        CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, OPEN_EXISTING,
        PIPE_ACCESS_OUTBOUND, ReadFile, WriteFile,
    },
    System::{
        IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED},
        JobObjects::*,
        Pipes::{
            ConnectNamedPipe, CreateNamedPipeW, CreatePipe, PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_TYPE_BYTE, PIPE_WAIT, PeekNamedPipe,
        },
        Threading::*,
    },
};

fn check(value: i32, context: &'static str) -> Result<()> {
    if value == 0 {
        Err(io::Error::last_os_error()).context(context)
    } else {
        Ok(())
    }
}
fn wide(value: &OsStr) -> Result<Vec<u16>> {
    let mut value: Vec<_> = value.encode_wide().collect();
    if value.contains(&0) {
        bail!("command contains a NUL character");
    }
    value.push(0);
    Ok(value)
}
fn handle(value: HANDLE) -> Result<OwnedHandle> {
    if value.is_null() || value == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error()).context("creating Windows handle");
    }
    Ok(unsafe { OwnedHandle::from_raw_handle(value) })
}
fn raw(value: &OwnedHandle) -> HANDLE {
    value.as_raw_handle()
}

fn pipe(child_reads: bool) -> Result<(OwnedHandle, OwnedHandle)> {
    let mut read = null_mut();
    let mut write = null_mut();
    let security = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: null_mut(),
        bInheritHandle: 1,
    };
    check(
        unsafe { CreatePipe(&mut read, &mut write, &security, 0) },
        "creating command pipe",
    )?;
    let read = handle(read)?;
    let write = handle(write)?;
    let (parent, child) = if child_reads {
        (write, read)
    } else {
        (read, write)
    };
    check(
        unsafe { SetHandleInformation(raw(&parent), HANDLE_FLAG_INHERIT, 0) },
        "disabling parent pipe inheritance",
    )?;
    Ok((parent, child))
}

struct Attributes {
    storage: Vec<usize>,
    initialized: bool,
}
impl Attributes {
    fn new() -> Result<Self> {
        let mut bytes = 0;
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), 2, 0, &mut bytes);
        }
        if bytes == 0 {
            bail!("Windows process attribute storage unavailable");
        }
        let mut list = Self {
            storage: vec![0; bytes.div_ceil(size_of::<usize>())],
            initialized: false,
        };
        check(
            unsafe { InitializeProcThreadAttributeList(list.pointer(), 2, 0, &mut bytes) },
            "initializing process attributes",
        )?;
        list.initialized = true;
        Ok(list)
    }
    fn pointer(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.storage.as_mut_ptr().cast()
    }
    fn set(&mut self, key: u32, values: &[HANDLE]) -> Result<()> {
        check(
            unsafe {
                UpdateProcThreadAttribute(
                    self.pointer(),
                    0,
                    key as usize,
                    values.as_ptr().cast(),
                    size_of_val(values),
                    null_mut(),
                    null(),
                )
            },
            "configuring process attributes",
        )
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        if self.initialized {
            unsafe {
                DeleteProcThreadAttributeList(self.pointer());
            }
        }
    }
}

struct Job {
    handle: OwnedHandle,
}
impl Job {
    fn new(resources: &ChildLimits) -> Result<Self> {
        if resources.file_bytes.is_some()
            || resources.open_files.is_some()
            || resources.address_space_bytes.is_some()
            || resources.core_bytes.is_some_and(|bytes| bytes != 0)
        {
            bail!(
                "Windows does not support configured file, open-file, address-space, or nonzero core limits"
            );
        }
        let job = Self {
            handle: handle(unsafe { CreateJobObjectW(null(), null()) })?,
        };
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION;
        if let Some(seconds) = resources.cpu_seconds {
            info.BasicLimitInformation.PerProcessUserTimeLimit = i64::try_from(
                seconds
                    .checked_mul(10_000_000)
                    .context("CPU limit overflow")?,
            )
            .context("CPU limit overflow")?;
            info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_PROCESS_TIME;
        }
        check(
            unsafe {
                SetInformationJobObject(
                    raw(&job.handle),
                    JobObjectExtendedLimitInformation,
                    (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    size_of_val(&info) as u32,
                )
            },
            "setting Windows job limits",
        )?;
        Ok(job)
    }
    fn terminate(&self) -> Result<()> {
        check(
            unsafe { TerminateJobObject(raw(&self.handle), 1) },
            "terminating command job",
        )
    }
    fn active(&self) -> Result<u32> {
        let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { std::mem::zeroed() };
        check(
            unsafe {
                QueryInformationJobObject(
                    raw(&self.handle),
                    JobObjectBasicAccountingInformation,
                    (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                    size_of_val(&info) as u32,
                    null_mut(),
                )
            },
            "observing command job",
        )?;
        Ok(info.ActiveProcesses)
    }
    fn finish(&self, process: &OwnedHandle) -> Result<ExitStatus> {
        self.terminate()?;
        let deadline = Instant::now() + PROCESS_CLEANUP_TIME;
        while self.active()? != 0 {
            if Instant::now() >= deadline {
                bail!("Windows job still had live processes at cleanup deadline");
            }
            thread::sleep(Duration::from_millis(2));
        }
        if unsafe { WaitForSingleObject(raw(process), 0) } != WAIT_OBJECT_0 {
            bail!("command process exit unavailable after job cleanup");
        }
        let mut code = 0;
        check(
            unsafe { GetExitCodeProcess(raw(process), &mut code) },
            "reading command exit status",
        )?;
        Ok(ExitStatus::from_raw(code))
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        let _ = self.terminate();
    }
}

// Microsoft CRT argument rules apply to the explicitly configured executable.
fn quote(argument: &OsStr, output: &mut Vec<u16>) -> Result<()> {
    let units: Vec<_> = argument.encode_wide().collect();
    if units.contains(&0) {
        bail!("command argument contains a NUL character");
    }
    output.push(b'"' as u16);
    let mut slashes = 0;
    for unit in units {
        if unit == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        output.extend(std::iter::repeat_n(
            b'\\' as u16,
            if unit == b'"' as u16 {
                slashes * 2 + 1
            } else {
                slashes
            },
        ));
        slashes = 0;
        output.push(unit);
    }
    output.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    output.push(b'"' as u16);
    Ok(())
}
fn executable(command: &Command) -> Result<std::path::PathBuf> {
    let program = Path::new(command.get_program());
    let cwd = command
        .get_current_dir()
        .map(Path::to_path_buf)
        .unwrap_or(std::env::current_dir()?);
    let candidates = if program.components().count() > 1 || program.is_absolute() {
        vec![cwd.join(program)]
    } else {
        let path = command
            .get_envs()
            .find(|(name, _)| name.eq_ignore_ascii_case("PATH"))
            .map(|(_, value)| value.map(OsStr::to_os_string))
            .unwrap_or_else(|| std::env::var_os("PATH"));
        let mut paths = vec![];
        if let Some(path) = path {
            paths.extend(
                std::env::split_paths(&path).map(|directory| cwd.join(directory).join(program)),
            );
        }
        paths
    };
    for path in candidates {
        let path = if path.extension().is_none() {
            path.with_extension("exe")
        } else {
            path
        };
        if path.is_file() {
            if path.extension().is_some_and(|ext| {
                ext.eq_ignore_ascii_case("cmd") || ext.eq_ignore_ascii_case("bat")
            }) {
                bail!("configure cmd.exe explicitly to execute Windows batch commands");
            }
            return std::fs::canonicalize(path).context("resolving command executable");
        }
    }
    bail!("configured executable {} was not found", program.display())
}
fn environment_key_order(left: &OsStr, right: &OsStr) -> Result<Ordering> {
    use windows_sys::Win32::Globalization::{
        CSTR_EQUAL, CSTR_GREATER_THAN, CSTR_LESS_THAN, CompareStringOrdinal,
    };
    let left: Vec<_> = left.encode_wide().collect();
    let right: Vec<_> = right.encode_wide().collect();
    let compared = unsafe {
        CompareStringOrdinal(
            left.as_ptr(),
            i32::try_from(left.len())?,
            right.as_ptr(),
            i32::try_from(right.len())?,
            1,
        )
    };
    match compared {
        CSTR_LESS_THAN => Ok(Ordering::Less),
        CSTR_EQUAL => Ok(Ordering::Equal),
        CSTR_GREATER_THAN => Ok(Ordering::Greater),
        _ => Err(io::Error::last_os_error()).context("comparing Windows environment names"),
    }
}
fn environment(command: &Command, inherit: bool) -> Result<Vec<u16>> {
    let mut values: Vec<(OsString, OsString)> = std::env::vars_os().filter(|_| inherit).collect();
    for (key, value) in command.get_envs() {
        let mut found = None;
        for (index, (existing, _)) in values.iter().enumerate() {
            if environment_key_order(existing, key)? == Ordering::Equal {
                found = Some(index);
                break;
            }
        }
        if let Some(index) = found {
            values.remove(index);
        }
        if let Some(value) = value {
            values.push((key.into(), value.into()));
        }
    }
    let mut compare_error = None;
    values.sort_by(
        |(left, _), (right, _)| match environment_key_order(left, right) {
            Ok(order) => order,
            Err(error) => {
                compare_error = Some(error);
                Ordering::Equal
            }
        },
    );
    if let Some(error) = compare_error {
        return Err(error);
    }
    let mut output = vec![];
    for (key, value) in values {
        let mut item = key;
        item.push("=");
        item.push(value);
        output.extend(wide(&item)?);
    }
    output.push(0);
    if output.len() == 1 {
        output.push(0);
    }
    Ok(output)
}
fn spawn(
    command: &Command,
    job: &Job,
    stdin: &OwnedHandle,
    stdout: &OwnedHandle,
    stderr: &OwnedHandle,
    inherit: bool,
) -> Result<OwnedHandle> {
    let application = wide(executable(command)?.as_os_str())?;
    let mut line = vec![];
    quote(command.get_program(), &mut line)?;
    for argument in command.get_args() {
        line.push(b' ' as u16);
        quote(argument, &mut line)?;
    }
    line.push(0);
    if line.len() > 32767 {
        bail!("Windows command line exceeds 32767 UTF-16 units");
    }
    let env = environment(command, inherit)?;
    let cwd = command
        .get_current_dir()
        .map(|path| {
            let path =
                std::fs::canonicalize(path).context("resolving command working directory")?;
            wide(path.as_os_str())
        })
        .transpose()?;
    let jobs = [raw(&job.handle)];
    let pipes = [raw(stdin), raw(stdout), raw(stderr)];
    let mut attributes = Attributes::new()?;
    attributes.set(PROC_THREAD_ATTRIBUTE_JOB_LIST, &jobs)?;
    attributes.set(PROC_THREAD_ATTRIBUTE_HANDLE_LIST, &pipes)?;
    let mut startup: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = raw(stdin);
    startup.StartupInfo.hStdOutput = raw(stdout);
    startup.StartupInfo.hStdError = raw(stderr);
    startup.lpAttributeList = attributes.pointer();
    let mut info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    // Job assignment is part of process creation. No executable can run outside
    // the owned job, even when Archguard terminates during process startup.
    check(
        unsafe {
            CreateProcessW(
                application.as_ptr(),
                line.as_mut_ptr(),
                null(),
                null(),
                1,
                EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW,
                env.as_ptr().cast(),
                cwd.as_ref().map_or(null(), |path| path.as_ptr()),
                &startup.StartupInfo,
                &mut info,
            )
        },
        "starting executable in Windows job",
    )?;
    unsafe {
        CloseHandle(info.hThread);
    }
    let process = handle(info.hProcess)?;
    let mut owned = 0;
    if unsafe { IsProcessInJob(raw(&process), raw(&job.handle), &mut owned) } == 0 || owned == 0 {
        let cleanup = job.finish(&process);
        bail!(
            "Windows child job ownership unavailable; cleanup: {}",
            cleanup
                .map(|_| "complete".to_owned())
                .unwrap_or_else(|error| format!("{error:#}"))
        );
    }
    Ok(process)
}

fn drain(
    pipe: &mut Option<OwnedHandle>,
    bytes: &mut Vec<u8>,
    limit: usize,
    name: &'static str,
) -> std::result::Result<(), CommandStop> {
    let Some(reader) = pipe.as_ref() else {
        return Ok(());
    };
    for _ in 0..16 {
        let mut available = 0;
        if unsafe {
            PeekNamedPipe(
                raw(reader),
                null_mut(),
                0,
                null_mut(),
                &mut available,
                null_mut(),
            )
        } == 0
        {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) {
                *pipe = None;
                return Ok(());
            }
            return Err(CommandStop::Io(format!("{name}: {error}")));
        }
        if available == 0 {
            return Ok(());
        }
        let mut buffer = [0u8; 8192];
        let remaining = limit.saturating_sub(bytes.len());
        let count = (available as usize)
            .min(buffer.len())
            .min(remaining.saturating_add(1));
        let mut read = 0;
        if unsafe {
            ReadFile(
                raw(reader),
                buffer.as_mut_ptr(),
                count as u32,
                &mut read,
                null_mut(),
            )
        } == 0
        {
            return Err(CommandStop::Io(format!(
                "{name}: {}",
                io::Error::last_os_error()
            )));
        }
        let read = read as usize;
        bytes.extend_from_slice(&buffer[..read.min(remaining)]);
        if read > remaining {
            return Err(CommandStop::OutputLimit(name));
        }
    }
    Ok(())
}

pub(super) fn run(
    command: Command,
    input: &[u8],
    limits: &CommandLimits,
    cancelled: impl Fn() -> bool,
    mut monitor: impl FnMut() -> Result<()>,
) -> Result<CommandOutput> {
    let mut output = CommandOutput {
        status: None,
        stdout: vec![],
        stderr: vec![],
        stop: CommandStop::Completed,
        stdin_complete: input.is_empty(),
        cleanup_complete: true,
    };
    if cancelled() || Instant::now() >= limits.deadline {
        output.stop = if cancelled() {
            CommandStop::Cancelled
        } else {
            CommandStop::TimedOut
        };
        return Ok(output);
    }
    let job = Job::new(&limits.resources)?;
    let (mut stdin, child_stdin) = InputPipe::new(input)?;
    let (stdout, child_stdout) = pipe(false)?;
    let (stderr, child_stderr) = pipe(false)?;
    let process = spawn(
        &command,
        &job,
        &child_stdin,
        &child_stdout,
        &child_stderr,
        limits.inherit_environment,
    )?;
    drop((child_stdin, child_stdout, child_stderr));
    output.cleanup_complete = false;
    let mut stdout = Some(stdout);
    let mut stderr = Some(stderr);
    let mut next_monitor = Instant::now();
    while output.stop == CommandStop::Completed {
        match stdin.step() {
            Ok(done) => output.stdin_complete = done,
            Err(error) => {
                output.stop = CommandStop::Io(format!("stdin: {error:#}"));
                break;
            }
        }
        if cancelled() {
            output.stop = CommandStop::Cancelled;
            break;
        }
        if Instant::now() >= limits.deadline {
            output.stop = CommandStop::TimedOut;
            break;
        }
        if Instant::now() >= next_monitor {
            if let Err(error) = monitor() {
                output.stop = CommandStop::Monitor(format!("{error:#}"));
                break;
            }
            next_monitor = Instant::now() + Duration::from_millis(20);
        }
        for (pipe, bytes, limit, name) in [
            (
                &mut stdout,
                &mut output.stdout,
                limits.stdout_bytes,
                "stdout",
            ),
            (
                &mut stderr,
                &mut output.stderr,
                limits.stderr_bytes,
                "stderr",
            ),
        ] {
            if let Err(stop) = drain(pipe, bytes, limit, name) {
                output.stop = stop;
                break;
            }
        }
        if output.stop != CommandStop::Completed {
            break;
        }
        match unsafe { WaitForSingleObject(raw(&process), 0) } {
            WAIT_OBJECT_0 => {
                break;
            }
            WAIT_TIMEOUT => {}
            _ => {
                output.stop = CommandStop::Io(format!(
                    "observing executable: {}",
                    io::Error::last_os_error()
                ));
                break;
            }
        }
        thread::sleep(Duration::from_millis(2));
    }
    match job.finish(&process) {
        Ok(status) => {
            output.status = Some(status);
            output.cleanup_complete = true;
        }
        Err(error) => output.stop = CommandStop::Cleanup(format!("{error:#}")),
    }
    if let Err(error) = stdin.finish() {
        output.cleanup_complete = false;
        output.stop = CommandStop::Cleanup(format!("{error:#}"));
    }
    output.stdin_complete |= stdin.complete();
    let drain_deadline = Instant::now() + PIPE_DRAIN_TIME;
    while stdout.is_some() || stderr.is_some() {
        for (pipe, bytes, limit, name) in [
            (
                &mut stdout,
                &mut output.stdout,
                limits.stdout_bytes,
                "stdout",
            ),
            (
                &mut stderr,
                &mut output.stderr,
                limits.stderr_bytes,
                "stderr",
            ),
        ] {
            if let Err(stop) = drain(pipe, bytes, limit, name) {
                if output.stop == CommandStop::Completed {
                    output.stop = stop;
                }
                *pipe = None;
            }
        }
        if Instant::now() >= drain_deadline {
            output.cleanup_complete = false;
            output.stop = CommandStop::Cleanup("Windows command pipes did not close".into());
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    if output.cleanup_complete
        && output.stop == CommandStop::Completed
        && let Err(error) = monitor()
    {
        output.stop = CommandStop::Monitor(format!("{error:#}"));
    }
    Ok(output)
}

struct InputPipe {
    pipe: Option<OwnedHandle>,
    event: OwnedHandle,
    overlap: Option<Box<OVERLAPPED>>,
    input: Option<Vec<u8>>,
    at: usize,
    pending: bool,
}
impl InputPipe {
    fn new(input: &[u8]) -> Result<(Self, OwnedHandle)> {
        use std::sync::atomic::{AtomicU64, Ordering};
        use windows_sys::Win32::Foundation::{
            ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, GENERIC_READ,
        };
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let name = wide(OsStr::new(&format!(
            r"\\.\pipe\archguard-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))?;
        let pipe = handle(unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_OUTBOUND | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                8192,
                8192,
                0,
                null(),
            )
        })?;
        let event = handle(unsafe { CreateEventW(null(), 1, 0, null()) })?;
        let mut overlap: Box<OVERLAPPED> = Box::new(unsafe { std::mem::zeroed() });
        overlap.hEvent = raw(&event);
        let connecting = unsafe { ConnectNamedPipe(raw(&pipe), &mut *overlap) };
        let connect_error = io::Error::last_os_error();
        if connecting == 0
            && connect_error.raw_os_error() != Some(ERROR_IO_PENDING as i32)
            && connect_error.raw_os_error() != Some(ERROR_PIPE_CONNECTED as i32)
        {
            return Err(connect_error).context("connecting command input pipe");
        }
        let security = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: null_mut(),
            bInheritHandle: 1,
        };
        let child = match handle(unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_READ,
                0,
                &security,
                OPEN_EXISTING,
                0,
                null_mut(),
            )
        }) {
            Ok(child) => child,
            Err(error) => {
                unsafe {
                    CancelIoEx(raw(&pipe), &*overlap);
                }
                if unsafe {
                    WaitForSingleObject(raw(&event), PROCESS_CLEANUP_TIME.as_millis() as u32)
                } != WAIT_OBJECT_0
                {
                    Box::leak(overlap);
                }
                return Err(error).context("opening command input pipe");
            }
        };
        let mut transferred = 0;
        check(
            unsafe { GetOverlappedResult(raw(&pipe), &*overlap, &mut transferred, 1) },
            "completing command input pipe connection",
        )?;
        check(
            unsafe { ResetEvent(raw(&event)) },
            "resetting command input event",
        )?;
        let mut writer = Self {
            pipe: Some(pipe),
            event,
            overlap: Some(overlap),
            input: Some(input.to_vec()),
            at: 0,
            pending: false,
        };
        if input.is_empty() {
            writer.pipe = None;
        }
        Ok((writer, child))
    }
    fn step(&mut self) -> Result<bool> {
        use windows_sys::Win32::Foundation::{ERROR_IO_INCOMPLETE, ERROR_IO_PENDING};
        let Some(pipe) = self.pipe.as_ref() else {
            return Ok(true);
        };
        let overlap = self.overlap.as_mut().unwrap();
        let input = self.input.as_ref().unwrap();
        if self.pending {
            let mut transferred = 0;
            if unsafe { GetOverlappedResult(raw(pipe), &**overlap, &mut transferred, 0) } == 0 {
                let error = io::Error::last_os_error();
                if error.raw_os_error() == Some(ERROR_IO_INCOMPLETE as i32) {
                    return Ok(false);
                }
                self.pending = false;
                return Err(error).context("delivering command input");
            }
            self.pending = false;
            self.at += transferred as usize;
        }
        if self.at == input.len() {
            self.pipe = None;
            return Ok(true);
        }
        check(
            unsafe { ResetEvent(raw(&self.event)) },
            "resetting command input event",
        )?;
        let count = (input.len() - self.at).min(8192);
        let written = unsafe {
            WriteFile(
                raw(pipe),
                input[self.at..].as_ptr(),
                count as u32,
                null_mut(),
                &mut **overlap,
            )
        };
        if written == 0
            && io::Error::last_os_error().raw_os_error() != Some(ERROR_IO_PENDING as i32)
        {
            return Err(io::Error::last_os_error()).context("writing command input");
        }
        self.pending = true;
        Ok(false)
    }
    fn complete(&self) -> bool {
        self.input
            .as_ref()
            .is_some_and(|input| self.at == input.len())
    }
    fn finish(&mut self) -> Result<()> {
        if self.pending {
            let pipe = self.pipe.as_ref().unwrap();
            let overlap = self.overlap.as_ref().unwrap();
            let mut transferred = 0;
            if unsafe { GetOverlappedResult(raw(pipe), &**overlap, &mut transferred, 0) } != 0 {
                self.at += transferred as usize;
                self.pending = false;
                self.pipe = None;
                return Ok(());
            }
            unsafe {
                CancelIoEx(raw(pipe), &**overlap);
            }
            if unsafe {
                WaitForSingleObject(raw(&self.event), PROCESS_CLEANUP_TIME.as_millis() as u32)
            } != WAIT_OBJECT_0
            {
                // Windows can still refer to these allocations after failed
                // cancellation. Retain them rather than freeing active IO memory.
                Box::leak(self.overlap.take().unwrap());
                self.input.take().unwrap().leak();
                self.pending = false;
                bail!("Windows input cancellation did not settle at cleanup deadline");
            }
            self.pending = false;
        }
        self.pipe = None;
        Ok(())
    }
}
impl Drop for InputPipe {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn limits() -> CommandLimits {
        CommandLimits {
            inherit_environment: true,
            deadline: Instant::now() + Duration::from_secs(3),
            stdin_bytes: 4 * 1024 * 1024,
            stdout_bytes: 1024,
            stderr_bytes: 1024,
            resources: ChildLimits::default(),
        }
    }
    fn node(program: &str) -> Command {
        let mut command = Command::new("node");
        command.args(["-e", program]);
        command
    }
    fn invoke(program: &str, input: &[u8], limits: &CommandLimits) -> CommandOutput {
        run_command(node(program), input, limits, || false, || Ok(())).unwrap()
    }
    #[test]
    fn bare_commands_do_not_discover_executables_in_scanned_directory() {
        let directory = tempfile::tempdir().unwrap();
        // A repository executable must not replace the configured PATH command.
        std::fs::copy(
            std::env::current_exe().unwrap(),
            directory.path().join("node.exe"),
        )
        .unwrap();
        let marker = directory.path().join("ran");
        let mut command = node(&format!(
            "require('node:fs').writeFileSync({:?},process.execPath)",
            marker.to_string_lossy()
        ));
        command.current_dir(directory.path());
        let resolved = executable(&command).unwrap();
        assert_ne!(
            resolved,
            std::fs::canonicalize(directory.path().join("node.exe")).unwrap()
        );
        let output = run_command(command, b"", &limits(), || false, || Ok(())).unwrap();
        assert!(output.status.unwrap().success() && output.cleanup_complete);
        assert_eq!(
            std::fs::canonicalize(std::fs::read_to_string(marker).unwrap()).unwrap(),
            resolved
        );
    }
    #[test]
    fn relative_path_entries_resolve_from_command_working_directory() {
        let directory = tempfile::tempdir().unwrap();
        let tools = directory.path().join("tools");
        std::fs::create_dir(&tools).unwrap();
        let real_node = executable(&node("process.exit(0)")).unwrap();
        let selected = tools.join("node.exe");
        std::fs::copy(real_node, &selected).unwrap();
        let mut command = node("process.stdout.write(process.execPath)");
        command.current_dir(directory.path()).env("PATH", "tools");
        let expected = std::fs::canonicalize(selected).unwrap();
        assert_eq!(executable(&command).unwrap(), expected);
        let output = run_command(command, b"", &limits(), || false, || Ok(())).unwrap();
        assert!(output.status.unwrap().success() && output.cleanup_complete);
        assert_eq!(
            std::fs::canonicalize(String::from_utf8(output.stdout).unwrap()).unwrap(),
            expected
        );
    }
    #[test]
    fn explicit_relative_working_directory_is_resolved_before_creation() {
        let mut command = node("process.stdout.write(process.cwd())");
        command.current_dir(".");
        let output = run_command(command, b"", &limits(), || false, || Ok(())).unwrap();
        assert!(output.status.unwrap().success() && output.cleanup_complete);
        assert_eq!(
            std::fs::canonicalize(String::from_utf8(output.stdout).unwrap()).unwrap(),
            std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap()
        );
    }
    #[test]
    fn complete_transport_and_argument_quoting() {
        let mut command = node(
            "process.stdin.resume();process.stdin.on('end',()=>process.stdout.write(JSON.stringify(process.argv.slice(1))))",
        );
        let arguments = [
            "",
            "space here",
            "quote\"here",
            "trail\\",
            "é你好",
            "double\\\\\"quote",
        ];
        command.args(arguments);
        let output = run_command(command, b"request", &limits(), || false, || Ok(())).unwrap();
        assert_eq!(output.stop, CommandStop::Completed);
        assert!(
            output.status.unwrap().success() && output.cleanup_complete && output.stdin_complete
        );
        assert_eq!(
            serde_json::from_slice::<Vec<String>>(&output.stdout).unwrap(),
            arguments
        );
    }
    #[test]
    fn output_floods_remain_bounded() {
        for stream in ["stdout", "stderr"] {
            let output = invoke(
                &format!("process.{stream}.write('x'.repeat(5000));setInterval(()=>{{}},1000)"),
                b"",
                &limits(),
            );
            assert_eq!(output.stop, CommandStop::OutputLimit(stream));
            assert!(
                output.stdout.len() <= 1024
                    && output.stderr.len() <= 1024
                    && output.cleanup_complete
            );
        }
    }
    #[test]
    fn unread_input_and_sleeping_children_have_bounded_deadlines() {
        let mut bound = limits();
        bound.deadline = Instant::now() + Duration::from_millis(250);
        let started = Instant::now();
        let output = invoke(
            "setInterval(()=>{},1000)",
            &vec![b'x'; 2 * 1024 * 1024],
            &bound,
        );
        assert_eq!(output.stop, CommandStop::TimedOut);
        assert!(!output.stdin_complete && output.cleanup_complete);
        assert!(started.elapsed() < Duration::from_secs(3));
    }
    #[test]
    fn cancellation_and_monitor_failure_clean_the_job() {
        let start = Instant::now();
        let output = run_command(
            node("setInterval(()=>{},1000)"),
            b"",
            &limits(),
            || start.elapsed() > Duration::from_millis(100),
            || Ok(()),
        )
        .unwrap();
        assert_eq!(output.stop, CommandStop::Cancelled);
        assert!(output.cleanup_complete);
        let output = run_command(
            node("setInterval(()=>{},1000)"),
            b"",
            &limits(),
            || false,
            || bail!("workspace cap"),
        )
        .unwrap();
        assert_eq!(output.stop, CommandStop::Monitor("workspace cap".into()));
        assert!(output.cleanup_complete);
    }
    #[test]
    fn cpu_limit_stops_a_busy_command_with_a_nonzero_status() {
        let mut bound = limits();
        bound.deadline = Instant::now() + Duration::from_secs(8);
        bound.resources.cpu_seconds = Some(1);
        let output = invoke("for(;;){}", b"", &bound);
        assert_eq!(output.stop, CommandStop::Completed);
        assert!(!output.status.unwrap().success() && output.cleanup_complete);
        let output = invoke("process.exit(0)", b"", &bound);
        assert!(output.status.unwrap().success() && output.cleanup_complete);
    }
    #[test]
    fn unsupported_limits_fail_before_any_command_runs() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("ran");
        for resource in [
            ChildLimits {
                file_bytes: Some(10),
                ..Default::default()
            },
            ChildLimits {
                open_files: Some(10),
                ..Default::default()
            },
            ChildLimits {
                address_space_bytes: Some(10),
                ..Default::default()
            },
        ] {
            let mut bound = limits();
            bound.resources = resource;
            let error = run_command(
                node(&format!(
                    "require('node:fs').writeFileSync({:?},'yes')",
                    marker.to_string_lossy()
                )),
                b"",
                &bound,
                || false,
                || Ok(()),
            )
            .err()
            .unwrap();
            assert!(error.to_string().contains("does not support"));
            assert!(!marker.exists());
        }
    }
    fn process_handle(pid: u32) -> OwnedHandle {
        handle(unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                pid,
            )
        })
        .unwrap()
    }
    #[test]
    fn root_exit_cleans_inherited_pipes_and_detached_descendants() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("pid");
        let mut child_handle = None;
        let program = format!(
            "const c=require('node:child_process').spawn(process.execPath,['-e','setInterval(()=>{{}},1000)'],{{detached:true,stdio:['ignore',1,2]}});require('node:fs').writeFileSync({:?},String(c.pid));setTimeout(()=>process.exit(0),200)",
            marker.to_string_lossy()
        );
        let output = run_command(
            node(&program),
            b"",
            &limits(),
            || false,
            || {
                if child_handle.is_none()
                    && let Ok(text) = std::fs::read_to_string(&marker)
                    && let Ok(pid) = text.parse()
                {
                    child_handle = Some(process_handle(pid));
                }
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(output.stop, CommandStop::Completed);
        assert!(output.status.unwrap().success() && output.cleanup_complete);
        assert_eq!(
            unsafe { WaitForSingleObject(raw(&child_handle.unwrap()), 1000) },
            WAIT_OBJECT_0
        );
    }
    #[test]
    fn unicode_environment_names_follow_windows_ordinal_comparison() {
        let mut command = node(
            "process.stdout.write(JSON.stringify([process.env['straße'],process.env.STRASSE,process.env.Mixed]))",
        );
        command
            .env_clear()
            .env("straße", "left")
            .env("STRASSE", "right")
            .env("mixed", "selected");
        let mut bound = limits();
        bound.inherit_environment = false;
        let output = run_command(command, b"", &bound, || false, || Ok(())).unwrap();
        assert_eq!(output.stop, CommandStop::Completed);
        assert!(output.status.unwrap().success() && output.cleanup_complete);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
            serde_json::json!(["left", "right", "selected"])
        );
    }
    #[test]
    fn explicit_environment_does_not_inherit_parent_variables() {
        let mut command = node(
            "process.stdout.write(JSON.stringify([process.env.PATH,process.env.ARCHGUARD_EXPLICIT]))",
        );
        command.env_clear().env("ARCHGUARD_EXPLICIT", "yes");
        let mut bound = limits();
        bound.inherit_environment = false;
        let output = run_command(command, b"", &bound, || false, || Ok(())).unwrap();
        assert_eq!(output.stop, CommandStop::Completed);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
            serde_json::json!([null, "yes"])
        );
    }
    #[test]
    fn parent_termination_closes_job_and_kills_commands() {
        const MARKER: &str = "ARCHGUARD_WINDOWS_JOB_PARENT_TEST";
        if let Some(marker) = std::env::var_os(MARKER) {
            let program = format!(
                "require('node:fs').writeFileSync({:?},String(process.pid));setInterval(()=>{{}},1000)",
                marker.to_string_lossy()
            );
            let mut bound = limits();
            bound.deadline = Instant::now() + Duration::from_secs(60);
            let _ = run_command(node(&program), b"", &bound, || false, || Ok(()));
            panic!("parent was expected to terminate");
        }
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("pid");
        let mut parent = Command::new(std::env::current_exe().unwrap())
            .args([
                "subprocess::windows::tests::parent_termination_closes_job_and_kills_commands",
                "--exact",
            ])
            .env(MARKER, &marker)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let child = loop {
            if let Ok(pid) = std::fs::read_to_string(&marker)
                && let Ok(pid) = pid.parse()
            {
                break process_handle(pid);
            }
            if Instant::now() >= deadline {
                let _ = parent.kill();
                let _ = parent.wait();
                panic!("child startup deadline");
            }
            thread::sleep(Duration::from_millis(10));
        };
        parent.kill().unwrap();
        parent.wait().unwrap();
        assert_eq!(
            unsafe { WaitForSingleObject(raw(&child), 5000) },
            WAIT_OBJECT_0
        );
    }
}
