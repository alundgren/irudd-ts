#![cfg(unix)]
use archguard::{
    config::{Config, PluginConfig},
    plugin, project,
};
use serde_json::json;
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};
fn facts(root: &Path) -> archguard::facts::ProjectFacts {
    fs::write(root.join("main.ts"), "export const x=1;").unwrap();
    project::analyze(
        root,
        &serde_json::from_value::<Config>(json!({"schemaVersion":1})).unwrap(),
    )
    .unwrap()
}
fn invoke(
    program: &str,
    timeout: u64,
) -> (anyhow::Result<Vec<archguard::facts::Diagnostic>>, Duration) {
    let root = tempfile::tempdir().unwrap();
    let f = facts(root.path());
    let p = PluginConfig {
        name: "test".into(),
        command: vec!["node".into(), "-e".into(), program.into()],
        timeout_ms: timeout,
    };
    let start = Instant::now();
    (plugin::run(&f, &p, root.path()), start.elapsed())
}
#[test]
fn plugin_failures_are_bounded_and_never_clean() {
    for program in [
        "setInterval(()=>{},1000)",
        "process.stderr.write('x'.repeat(100000));setInterval(()=>{},1000)",
        "process.exit(4)",
        "process.stdout.write('{}')",
        "process.stdout.write(JSON.stringify({schemaVersion:2,diagnostics:[]}))",
        "process.stdout.write(JSON.stringify({schemaVersion:1,diagnostics:[{rule:'x',file:'missing.ts',offset:0,message:'x'}]}))",
        "process.stdout.write('{}{}')",
        "process.stdout.write('x'.repeat(9000000))",
    ] {
        let (result, elapsed) = invoke(program, 500);
        assert!(result.is_err(), "{program}");
        assert!(elapsed < Duration::from_secs(3), "{program}: {elapsed:?}");
    }
}
#[test]
fn successful_plugin_and_source_range_validation() {
    let (result, _) = invoke(
        "process.stdin.resume();process.stdin.on('end',()=>process.stdout.write(JSON.stringify({schemaVersion:1,diagnostics:[{rule:'x',file:'main.ts',offset:0,message:'x'}]})))",
        1500,
    );
    assert_eq!(result.unwrap().len(), 1);
    let (result, _) = invoke(
        "process.stdout.write(JSON.stringify({schemaVersion:1,diagnostics:[{rule:'x',file:'main.ts',offset:99999,message:'x'}]}))",
        1500,
    );
    assert!(result.is_err());
}
#[test]
fn unread_large_stdin_times_out() {
    let root = tempfile::tempdir().unwrap();
    let mut f = facts(root.path());
    f.root = "x".repeat(2_000_000);
    let p = PluginConfig {
        name: "blocked".into(),
        command: vec![
            "node".into(),
            "-e".into(),
            "setInterval(()=>{},1000)".into(),
        ],
        timeout_ms: 250,
    };
    let start = Instant::now();
    assert!(plugin::run(&f, &p, root.path()).is_err());
    assert!(start.elapsed() < Duration::from_secs(3));
}
#[test]
fn inherited_pipes_are_closed_by_process_group_cleanup() {
    let (result, elapsed) = invoke(
        "require('child_process').spawn(process.execPath,['-e','setInterval(()=>{},1000)'],{stdio:['ignore',1,2]});process.stdout.write(JSON.stringify({schemaVersion:1,diagnostics:[]}));process.exit(0)",
        1000,
    );
    assert!(result.is_ok(), "{result:?}");
    assert!(elapsed < Duration::from_secs(2));
}
#[test]
fn rust_and_typescript_plugins_use_the_same_transitive_graph() {
    use archguard::{config::RuleConfig, facts::ProjectRule};
    let root = tempfile::tempdir().unwrap();
    for (path, source) in [
        ("client/main.ts", "import '../shared/helper.ts';"),
        ("shared/helper.ts", "import '../server/db.ts';"),
        ("server/db.ts", "export const db=1;"),
    ] {
        let path = root.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
    }
    let facts = project::analyze(
        root.path(),
        &serde_json::from_value::<Config>(json!({"schemaVersion":1})).unwrap(),
    )
    .unwrap();
    let rule:RuleConfig=serde_json::from_value(json!({"id":"client-server","kind":"forbiddenDependency","files":["client/**"],"targets":["server/**"],"transitive":true})).unwrap();
    let rust = rule.check(&facts).unwrap();
    let config = PluginConfig {
        name: "graph-parity".into(),
        command: vec![
            "node".into(),
            format!("{}/examples/graph-plugin.ts", env!("CARGO_MANIFEST_DIR")),
        ],
        timeout_ms: 3000,
    };
    let ts = plugin::run(&facts, &config, root.path()).unwrap();
    assert_eq!(rust, ts);
    assert_eq!(rust.len(), 1);
}
