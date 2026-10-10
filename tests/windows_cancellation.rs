#![cfg(windows)]
use serde_json::json;
use std::{
    fs,
    os::windows::{
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
        process::CommandExt,
    },
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::WAIT_OBJECT_0,
    System::{
        Console::{
            AllocConsole, CTRL_BREAK_EVENT, FreeConsole, GenerateConsoleCtrlEvent, GetStdHandle,
            STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
        },
        Threading::{
            CREATE_NEW_PROCESS_GROUP, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
            PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, TerminateProcess, WaitForSingleObject,
        },
    },
};

fn collect(child: std::process::Child, timeout: Duration) -> Output {
    let process = unsafe { OpenProcess(PROCESS_TERMINATE, 0, child.id()) };
    assert!(!process.is_null());
    let process = unsafe { OwnedHandle::from_raw_handle(process) };
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    thread::spawn(move || {
        let _ = sender.send(child.wait_with_output());
    });
    match receiver.recv_timeout(timeout) {
        Ok(output) => output.unwrap(),
        Err(error) => {
            unsafe {
                TerminateProcess(process.as_raw_handle(), 1);
            }
            let _ = receiver.recv_timeout(Duration::from_secs(5));
            panic!("Windows CLI cancellation did not complete before deadline: {error}");
        }
    }
}
#[test]
fn console_break_cancels_mutation_and_cleans_commands() {
    const HELPER: &str = "ARCHGUARD_WINDOWS_CONSOLE_HELPER";
    if std::env::var_os(HELPER).is_none() {
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "console_break_cancels_mutation_and_cleans_commands",
                "--exact",
                "--nocapture",
            ])
            .env(HELPER, "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let output = collect(child, Duration::from_secs(90));
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    // An isolated helper owns a fresh console so the targeted event cannot
    // cancel another test, the development terminal, or the release runner.
    let streams = [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE]
        .map(|stream| (stream, unsafe { GetStdHandle(stream) }));
    unsafe {
        FreeConsole();
    }
    assert_ne!(
        unsafe { AllocConsole() },
        0,
        "{}",
        std::io::Error::last_os_error()
    );
    for (stream, handle) in streams {
        assert_ne!(unsafe { SetStdHandle(stream, handle) }, 0);
    }
    struct ConsoleGuard;
    impl Drop for ConsoleGuard {
        fn drop(&mut self) {
            unsafe {
                FreeConsole();
            }
        }
    }
    let _console = ConsoleGuard;
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let source = "export const enabled = true;";
    fs::write(root.path().join("domain.ts"), source).unwrap();
    fs::write(root.path().join("wait.mjs"), "import fs from 'node:fs'; fs.writeFileSync(process.argv[2], String(process.pid)); setInterval(()=>{},1000);").unwrap();
    let node = Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .unwrap();
    assert!(node.status.success());
    let node = String::from_utf8(node.stdout).unwrap().trim().to_owned();
    let marker = external.path().join("ready");
    let config = external.path().join("mutator.json");
    fs::write(&config, json!({"schemaVersion":1,"plan":{"schemaVersion":1,"selection":{"include":["domain.ts"]}},"execution":{"command":[node,"wait.mjs",marker],"workspace":{"include":["domain.ts","wait.mjs"]},"limits":{"commandTimeoutMs":60000,"runTimeoutMs":120000}}}).to_string()).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_archguard"))
        .args(["mutator", "run", "--json", "--root"])
        .arg(root.path())
        .arg("--config")
        .arg(config)
        .creation_flags(CREATE_NEW_PROCESS_GROUP)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    let pid = loop {
        if let Ok(pid) = fs::read_to_string(&marker)
            && let Ok(pid) = pid.parse::<u32>()
        {
            break pid;
        }
        if child.try_wait().unwrap().is_some() {
            panic!("mutation CLI exited before command startup");
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("mutation command startup timed out");
        }
        thread::sleep(Duration::from_millis(10));
    };
    let process = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            pid,
        )
    };
    assert!(!process.is_null());
    let process = unsafe { OwnedHandle::from_raw_handle(process) };
    assert_ne!(
        unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, child.id()) },
        0,
        "{}",
        std::io::Error::last_os_error()
    );
    let output = collect(child, Duration::from_secs(15));
    assert_eq!(
        output.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["complete"], false);
    assert_eq!(report["baseline"]["outcome"], "cancelled");
    assert_eq!(
        unsafe { WaitForSingleObject(process.as_raw_handle(), 5000) },
        WAIT_OBJECT_0
    );
    assert_eq!(
        fs::read_to_string(root.path().join("domain.ts")).unwrap(),
        source
    );
}
