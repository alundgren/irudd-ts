use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn invoke(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_archguard"))
        .args(args)
        .arg("--root")
        .arg(root)
        .output()
        .unwrap()
}
fn report(output: &Output, code: i32) -> Value {
    assert_eq!(
        output.status.code(),
        Some(code),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn invalid_source_is_incomplete_and_correction_reports_clones_without_a_gate() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("functions.ts");
    fs::write(&file, "export function broken( {").unwrap();
    let bad = report(&invoke(root.path(), &["dryer", "--json"]), 2);
    assert_eq!(bad["complete"], false);
    assert!(!bad["problems"].as_array().unwrap().is_empty());
    fs::write(
        &file,
        r#"
function invoice(value: number) {
  const amount = Math.round(value);
  if (amount > 100) return amount + 5;
  return amount;
}
function order(input: number) {
  const total = Math.round(input);
  if (total > 200) return total + 10;
  return total;
}
"#,
    )
    .unwrap();
    let good = report(&invoke(root.path(), &["dryer", "--json"]), 0);
    assert_eq!(good["complete"], true);
    assert_eq!(good["pairs"].as_array().unwrap().len(), 1);
    assert_eq!(good["pairs"][0]["similarity"]["set"], 1.0);
    fs::write(&file, "export const unrelated = 5;").unwrap();
    let negative = report(&invoke(root.path(), &["dryer", "--json"]), 0);
    assert!(negative["pairs"].as_array().unwrap().is_empty());
}

#[test]
fn planning_never_executes_repository_commands_and_retains_runtime_sites() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("package.json"),
        r#"{"scripts":{"test":"touch unexpected"}}"#,
    )
    .unwrap();
    fs::write(root.path().join("domain.ts"), "type Flag = true; export function eligible(amount:number){ return amount >= 100 && true; }").unwrap();
    let planned = report(&invoke(root.path(), &["mutator", "plan", "--json"]), 0);
    assert_eq!(planned["complete"], true);
    let expected: Vec<_> = planned["sites"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["expected"].as_str().unwrap())
        .collect();
    assert_eq!(expected, [">=", "&&", "true"]);
    assert!(!root.path().join("unexpected").exists());
    fs::write(root.path().join("domain.ts"), "export function invalid(").unwrap();
    let incomplete = report(&invoke(root.path(), &["mutator", "plan", "--json"]), 2);
    assert_eq!(incomplete["complete"], false);
    assert!(!root.path().join("unexpected").exists());
}

#[test]
fn invalid_configuration_is_rejected_and_corrected_selection_is_explicit() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("selected.ts"), "export const flag = true;").unwrap();
    fs::write(root.path().join("other.ts"), "export const flag = false;").unwrap();
    let configuration = root.path().join("plan.json");
    fs::write(
        &configuration,
        json!({"schemaVersion":1,"unexpected":true}).to_string(),
    )
    .unwrap();
    let output = invoke(
        root.path(),
        &[
            "mutator",
            "plan",
            "--config",
            configuration.to_str().unwrap(),
            "--json",
        ],
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    fs::write(
        &configuration,
        json!({"schemaVersion":1,"selection":{"include":["selected.ts"]}}).to_string(),
    )
    .unwrap();
    let selected = report(
        &invoke(
            root.path(),
            &[
                "mutator",
                "plan",
                "--config",
                configuration.to_str().unwrap(),
                "--json",
            ],
        ),
        0,
    );
    assert_eq!(selected["files"].as_array().unwrap().len(), 1);
    assert_eq!(selected["files"][0]["path"], "selected.ts");
    fs::write(&configuration, " ".repeat(16_385)).unwrap();
    assert_eq!(
        invoke(
            root.path(),
            &[
                "mutator",
                "plan",
                "--config",
                configuration.to_str().unwrap()
            ]
        )
        .status
        .code(),
        Some(2)
    );
}

#[cfg(unix)]
#[test]
fn configuration_links_and_fifos_fail_without_blocking() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("config.json");
    fs::write(&config, r#"{"schemaVersion":1}"#).unwrap();
    let link = root.path().join("link.json");
    symlink(&config, &link).unwrap();
    assert_eq!(
        invoke(root.path(), &["dryer", "--config", link.to_str().unwrap()])
            .status
            .code(),
        Some(2)
    );
    let fifo = root.path().join("fifo.json");
    assert!(
        Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(
        invoke(root.path(), &["dryer", "--config", fifo.to_str().unwrap()])
            .status
            .code(),
        Some(2)
    );
    fs::remove_file(&fifo).unwrap();
    fs::remove_file(&link).unwrap();
    fs::write(root.path().join("clean.ts"), "export const value = 5;").unwrap();
    assert_eq!(
        invoke(
            root.path(),
            &["dryer", "--config", config.to_str().unwrap()]
        )
        .status
        .code(),
        Some(0)
    );
}
