use archguard::mutator::{
    self, BaselineOutcome, CancellationToken, ExecutionConfig, ExecutionProblemKind,
    MutationOutcome, MutationPlanConfig, ReusePolicy, StateConfig,
};
use std::{fs, path::PathBuf};
use tempfile::TempDir;

struct Fixture {
    directory: TempDir,
    root: PathBuf,
    config: ExecutionConfig,
    plan: mutator::MutationPlan,
    marker: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("input");
        fs::create_dir(&root).unwrap();
        for (name, source) in [
            (
                "subject.ts",
                include_str!("fixtures/mutation_execution/subject.ts"),
            ),
            (
                "runner.mjs",
                include_str!("fixtures/mutation_execution/runner.mjs"),
            ),
        ] {
            fs::write(root.join(name), source).unwrap();
        }
        fs::write(root.join("control.json"), r#"{"mode":"pass"}"#).unwrap();
        let marker = directory.path().join("calls.txt");
        let mut config: ExecutionConfig =
            serde_json::from_str(r#"{"command":["node","runner.mjs"]}"#).unwrap();
        let actual_node = std::process::Command::new("node")
            .args(["-p", "process.execPath"])
            .output()
            .unwrap();
        assert!(actual_node.status.success());
        config.command[0] = String::from_utf8(actual_node.stdout).unwrap().trim().into();
        config.command.push(marker.to_str().unwrap().into());
        config.state = Some(StateConfig {
            directory: directory.path().join("state"),
            reuse: ReusePolicy::DeclaredInputs,
            external_inputs: vec![],
        });
        let mut plan_config = MutationPlanConfig::default();
        plan_config.selection.include = vec!["subject.ts".into()];
        plan_config.operators = vec![mutator::MutationOperator::Boolean];
        let plan = mutator::plan(&root, &plan_config).unwrap();
        assert!(plan.complete);
        assert_eq!(plan.sites.len(), 2);
        Self {
            directory,
            root,
            config,
            plan,
            marker,
        }
    }
    fn mode(&self, mode: &str) {
        fs::write(
            self.root.join("control.json"),
            serde_json::to_vec(&serde_json::json!({"mode":mode})).unwrap(),
        )
        .unwrap();
    }
    fn run(&self) -> mutator::MutationReport {
        mutator::run(
            &self.plan,
            &self.config,
            self.directory.path(),
            &CancellationToken::new(),
        )
        .unwrap()
    }
    fn calls(&self) -> Vec<String> {
        fs::read_to_string(&self.marker)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}
#[test]
fn fresh_baseline_resume_invalidation_and_source_preservation() {
    let fixture = Fixture::new();
    let original = fs::read(fixture.root.join("subject.ts")).unwrap();
    let first = fixture.run();
    assert!(first.complete, "{:?}", first.problems);
    assert_eq!(first.summary.killed, 1);
    assert_eq!(first.summary.survived, 1);
    assert_eq!(first.summary.reused, 0);
    assert_eq!(fixture.calls(), vec!["baseline", "mutation", "mutation"]);
    let second = fixture.run();
    assert!(second.complete, "{:?}", second.problems);
    assert_eq!(second.summary.reused, 2);
    assert_eq!(fixture.calls().last().unwrap(), "baseline");
    assert_eq!(fixture.calls().len(), 4);
    assert_eq!(first.input_digest, second.input_digest);
    let record = fs::read_dir(
        fixture
            .config
            .state
            .as_ref()
            .unwrap()
            .directory
            .join("records"),
    )
    .unwrap()
    .next()
    .unwrap()
    .unwrap()
    .path();
    fs::write(record, b"corrupt").unwrap();
    let recovered = fixture.run();
    assert!(recovered.complete, "{:?}", recovered.problems);
    assert_eq!(recovered.summary.reused, 1);
    assert!(
        recovered
            .execution
            .reuse_reason
            .as_ref()
            .unwrap()
            .contains("Rejected 1")
    );
    fs::write(fixture.root.join("helper.txt"), "new input").unwrap();
    let third = fixture.run();
    assert!(third.complete, "{:?}", third.problems);
    assert_eq!(third.summary.reused, 0);
    assert_ne!(first.input_digest, third.input_digest);
    assert_eq!(fs::read(fixture.root.join("subject.ts")).unwrap(), original);
    fixture.mode("baselineFailure");
    let failed = fixture.run();
    assert!(!failed.complete);
    assert_eq!(failed.baseline.outcome, BaselineOutcome::Failed);
    assert_eq!(failed.summary.not_run, 2);
    assert_eq!(fixture.calls().last().unwrap(), "baseline");
}
#[test]
fn parallel_workers_and_rewritten_copies_keep_independent_outcomes() {
    let mut fixture = Fixture::new();
    fixture.config.limits.workers = 2;
    fixture.config.state.as_mut().unwrap().reuse = ReusePolicy::Off;
    fixture.mode("rewrite");
    let report = fixture.run();
    assert!(report.complete, "{:?}", report.problems);
    assert_eq!(report.summary.killed, 1);
    assert_eq!(report.summary.survived, 1);
    assert_eq!(
        fs::read_to_string(fixture.root.join("control.json")).unwrap(),
        r#"{"mode":"rewrite"}"#
    );
    assert_eq!(
        fs::read_to_string(fixture.root.join("subject.ts")).unwrap(),
        include_str!("fixtures/mutation_execution/subject.ts")
    );
    assert!(
        fs::read_dir(
            fixture
                .config
                .state
                .as_ref()
                .unwrap()
                .directory
                .join("records")
        )
        .unwrap()
        .count()
            > 0
    );
}
#[test]
fn runtime_protocol_failures_never_count_as_kills() {
    let mut fixture = Fixture::new();
    fixture.config.state.as_mut().unwrap().reuse = ReusePolicy::Off;
    for mode in [
        "mixed",
        "stale",
        "missing",
        "arbitrary",
        "zero",
        "provisional",
    ] {
        fixture.mode(mode);
        let report = fixture.run();
        assert!(!report.complete, "{mode}");
        assert_eq!(report.summary.killed, 0, "{mode}");
        assert!(
            report
                .results
                .iter()
                .any(|result| result.outcome == MutationOutcome::ExecutionError),
            "{mode}: {:?}",
            report.problems
        );
    }
    fixture.mode("errorEnabled");
    let serial = fixture.run();
    assert_eq!(serial.summary.execution_errors, 1);
    assert_eq!(serial.summary.survived, 1);
    fixture.config.limits.workers = 2;
    let parallel = fixture.run();
    assert_eq!(
        parallel
            .results
            .iter()
            .map(|result| result.outcome)
            .collect::<Vec<_>>(),
        serial
            .results
            .iter()
            .map(|result| result.outcome)
            .collect::<Vec<_>>()
    );
    fixture.mode("pass");
    assert!(fixture.run().complete);
}
#[test]
fn cancelled_or_timed_out_commands_cleanup_and_allow_fresh_resume() {
    let mut fixture = Fixture::new();
    fixture.config.state.as_mut().unwrap().reuse = ReusePolicy::Off;
    fixture.config.limits.command_timeout_ms = 2000;
    fixture.mode("wait");
    let report = fixture.run();
    assert!(!report.complete);
    assert_eq!(report.summary.timed_out, 2);
    assert!(
        report
            .problems
            .iter()
            .any(|problem| problem.kind == ExecutionProblemKind::Limit)
    );
    assert!(
        !fixture
            .config
            .state
            .as_ref()
            .unwrap()
            .directory
            .join("active.json")
            .exists()
    );
    let token = CancellationToken::new();
    let cancel = token.clone();
    let plan = &fixture.plan;
    let config = &fixture.config;
    let directory = fixture.directory.path();
    let report = std::thread::scope(|scope| {
        scope.spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(250));
            cancel.cancel();
        });
        mutator::run(plan, config, directory, &token).unwrap()
    });
    assert!(!report.complete);
    assert!(
        report
            .problems
            .iter()
            .any(|problem| problem.kind == ExecutionProblemKind::Cancellation)
    );
    fixture.mode("pass");
    fixture.config.limits.command_timeout_ms = 120000;
    assert!(fixture.run().complete);
}

#[test]
fn earlier_deadline_prevents_commands_without_changing_config_identity() {
    let fixture = Fixture::new();
    let report = mutator::run_until(
        &fixture.plan,
        &fixture.config,
        fixture.directory.path(),
        &CancellationToken::new(),
        std::time::Instant::now(),
    )
    .unwrap();
    assert!(!report.complete);
    assert_eq!(report.baseline.outcome, BaselineOutcome::NotRun);
    assert!(!fixture.marker.exists());
    assert!(
        report
            .problems
            .iter()
            .any(|problem| problem.kind == ExecutionProblemKind::Limit)
    );
    assert!(fixture.run().complete);
}

#[test]
fn completed_checkpoint_survives_cancel_and_resumes_after_fresh_baseline() {
    let fixture = Fixture::new();
    fixture.mode("waitUnusedOnce");
    let token = CancellationToken::new();
    let cancel = token.clone();
    let waiting = fixture.marker.with_extension("txt.waiting");
    let records = fixture
        .config
        .state
        .as_ref()
        .unwrap()
        .directory
        .join("records");
    let report = std::thread::scope(|scope| {
        scope.spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
            while !waiting.exists()
                || fs::read_dir(&records).map_or(true, |mut entries| entries.next().is_none())
            {
                assert!(
                    std::time::Instant::now() < deadline,
                    "second mutant never started"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            cancel.cancel();
        });
        mutator::run(
            &fixture.plan,
            &fixture.config,
            fixture.directory.path(),
            &token,
        )
        .unwrap()
    });
    assert!(!report.complete);
    assert_eq!(report.summary.killed, 1);
    assert_eq!(report.summary.cancelled, 1);
    fs::write(fixture.marker.with_extension("txt.released"), "release").unwrap();
    let before = fixture.calls().len();
    let resumed = fixture.run();
    assert!(resumed.complete, "{:?}", resumed.problems);
    assert_eq!(resumed.summary.reused, 1);
    assert_eq!(&fixture.calls()[before..], &["baseline", "mutation"]);
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires a release build and an exclusive execution pressure window"]
fn repeated_linux_execution_resources_remain_bounded() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    fn snapshot() -> (usize, usize, u64, Vec<u32>) {
        let fd = fs::read_dir("/proc/self/fd").unwrap().count();
        let tasks: Vec<_> = fs::read_dir("/proc/self/task")
            .unwrap()
            .map(Result::unwrap)
            .collect();
        let mut children = Vec::new();
        for task in &tasks {
            if let Ok(text) = fs::read_to_string(task.path().join("children")) {
                children.extend(
                    text.split_whitespace()
                        .map(|pid| pid.parse::<u32>().unwrap()),
                );
            }
        }
        children.sort_unstable();
        children.dedup();
        let status = fs::read_to_string("/proc/self/status").unwrap();
        let rss = status
            .lines()
            .find_map(|line| line.strip_prefix("VmRSS:"))
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        (fd, tasks.len(), rss, children)
    }
    let mut fixture = Fixture::new();
    fixture.config.limits.workers = 2;
    fixture.config.limits.max_workspace_file_bytes = 16_384;
    fixture.config.limits.max_workspace_bytes = 65_536;
    fixture.config.limits.max_total_workspace_bytes = 196_608;
    let original = fs::read(fixture.root.join("subject.ts")).unwrap();
    assert!(fixture.run().complete);
    let start = snapshot();
    let stop = Arc::new(AtomicBool::new(false));
    let monitor_stop = stop.clone();
    let monitor = std::thread::spawn(move || {
        let mut peak = (0, 0, 0, 0, 0_u64);
        while !monitor_stop.load(Ordering::Relaxed) {
            let (fd, threads, rss, children) = snapshot();
            peak.0 = peak.0.max(fd);
            peak.1 = peak.1.max(threads);
            peak.2 = peak.2.max(rss);
            peak.3 = peak.3.max(children.len());
            for pid in children {
                if let Ok(status) = fs::read_to_string(format!("/proc/{pid}/status"))
                    && let Some(rss) = status.lines().find_map(|line| line.strip_prefix("VmRSS:"))
                    && let Some(value) = rss
                        .split_whitespace()
                        .next()
                        .and_then(|value| value.parse::<u64>().ok())
                {
                    peak.4 = peak.4.max(value);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        peak
    });
    for round in 0..3 {
        fixture.config.state.as_mut().unwrap().reuse = ReusePolicy::Off;
        for mode in ["rewrite", "stdout", "disk", "wait"] {
            fixture.mode(mode);
            fixture.config.limits.command_timeout_ms = if mode == "wait" { 1000 } else { 120000 };
            let report = fixture.run();
            if mode == "rewrite" {
                assert!(report.complete, "{round} {mode}: {:?}", report.problems);
            } else {
                assert!(!report.complete, "{round} {mode}: {:?}", report.problems);
            }
            assert_eq!(fs::read(fixture.root.join("subject.ts")).unwrap(), original);
            let state = &fixture.config.state.as_ref().unwrap().directory;
            assert!(!state.join("active.json").exists());
            assert!(
                fs::read_dir(state)
                    .unwrap()
                    .map(Result::unwrap)
                    .all(|entry| !entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with("archguard-mutator"))
            );
        }
        fixture.config.limits.command_timeout_ms = 120000;
        fixture.mode("wait");
        let token = CancellationToken::new();
        let cancel = token.clone();
        let calls = fixture.calls().len();
        let marker = fixture.marker.clone();
        let cancelled = std::thread::scope(|scope| {
            scope.spawn(move || {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
                loop {
                    let count = fs::read_to_string(&marker)
                        .unwrap_or_default()
                        .lines()
                        .count();
                    if count >= calls + 2 {
                        break;
                    }
                    assert!(std::time::Instant::now() < deadline);
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                cancel.cancel();
            });
            mutator::run(
                &fixture.plan,
                &fixture.config,
                fixture.directory.path(),
                &token,
            )
            .unwrap()
        });
        assert!(!cancelled.complete);
        assert!(
            cancelled
                .problems
                .iter()
                .any(|problem| problem.kind == ExecutionProblemKind::Cancellation)
        );
        fixture.mode("pass");
        fixture.config.state.as_mut().unwrap().reuse = ReusePolicy::DeclaredInputs;
        assert!(fixture.run().complete);
        assert_eq!(fixture.run().summary.reused, 2);
    }
    stop.store(true, Ordering::Relaxed);
    let peaks = monitor.join().unwrap();
    let end = snapshot();
    assert_eq!(end.0, start.0, "file descriptors retained");
    assert_eq!(end.1, start.1, "threads retained");
    assert_eq!(end.3, start.3, "child processes retained");
    assert!(
        end.2 <= start.2 + 262_144,
        "RSS grew beyond 256 MiB allowance"
    );
    let state = &fixture.config.state.as_ref().unwrap().directory;
    assert!(!state.join("active.json").exists());
    let records: Vec<_> = fs::read_dir(state.join("records"))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let retained: u64 = records
        .iter()
        .map(|record| record.metadata().unwrap().len())
        .sum();
    assert!(retained < 1_048_576);
    println!(
        "Linux repeated execution: start={start:?} end={end:?} peak(fd,threads,parent_rss_kib,children,child_rss_kib)={peaks:?}; intentional_records={} bytes={retained}; workspace_limit=65536 total_workspace_limit=196608; no surviving owned workspace or active journal",
        records.len()
    );
}
