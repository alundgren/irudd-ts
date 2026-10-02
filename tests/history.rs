use serde_json::Value;
use std::{path::Path, process::Command};

fn check(root: &Path) -> (i32, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_archguard"))
        .args(["check", "--json", "--root"])
        .arg(root)
        .arg("--config")
        .arg(root.join("archguard.json"))
        .output()
        .unwrap();
    assert!(output.stderr.is_empty(), "{:?}", output);
    (
        output.status.code().unwrap(),
        serde_json::from_slice(&output.stdout).unwrap(),
    )
}

#[test]
fn upstream_reductions_fail_before_and_pass_after() {
    let history = Path::new(env!("CARGO_MANIFEST_DIR")).join("benchmarks/history");
    for id in ["14385", "14387", "14389", "13151"] {
        for revision in ["before", "fixed"] {
            let (status, report) = check(&history.join(id).join(revision));
            assert_eq!(report["complete"], true, "{id}/{revision}: {report}");
            assert_eq!(report["problems"].as_array().unwrap().len(), 0);
            let expected = usize::from(revision == "before");
            assert_eq!(
                report["diagnostics"].as_array().unwrap().len(),
                expected,
                "{id}/{revision}: {report}"
            );
            assert_eq!(status, i32::from(revision == "before"));
        }
    }
    let (status, report) = check(&history.join("controls"));
    assert_eq!(status, 0, "{report}");
    assert_eq!(report["complete"], true);
    assert_eq!(report["diagnostics"], serde_json::json!([]));
}

#[test]
fn type_only_cycles_and_dynamic_return_edges_have_distinct_policy() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("benchmarks/history/13151/before");
    let mut config: archguard::config::Config =
        serde_json::from_slice(&std::fs::read(fixture.join("archguard.json")).unwrap()).unwrap();
    config.rules[0].include_types = false;
    let facts = archguard::project::analyze(&fixture, &config).unwrap();
    assert!(facts.problems.is_empty());
    assert!(archguard::rules::check(&facts, &config).unwrap().is_empty());
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("a.ts"), "import './b.ts';").unwrap();
    std::fs::write(root.path().join("b.ts"), "import('./a.ts');").unwrap();
    config.rules[0].include_types = true;
    let facts = archguard::project::analyze(root.path(), &config).unwrap();
    assert!(facts.problems.is_empty());
    assert!(archguard::rules::check(&facts, &config).unwrap().is_empty());
}

#[test]
fn mobile_cycle_reduction_matches_both_upstream_platform_orders() {
    use archguard::facts::ProjectRule;
    let history = Path::new(env!("CARGO_MANIFEST_DIR")).join("benchmarks/history/13151");
    let modal = "apps/mobile/src/components/FilePreviewModal.tsx";
    for platform in ["android", "ios"] {
        let expected_target = if platform == "ios" {
            "apps/mobile/src/components/FilePreview.ios.tsx"
        } else {
            "apps/mobile/src/components/FilePreview.tsx"
        };
        for revision in ["before", "fixed"] {
            let root = history.join(revision);
            let config: archguard::config::Config = serde_json::from_slice(
                &std::fs::read(root.join(format!("archguard.{platform}.json"))).unwrap(),
            )
            .unwrap();
            config.validate().unwrap();
            let facts = archguard::project::analyze(&root, &config).unwrap();
            assert!(
                facts.problems.is_empty(),
                "{platform}/{revision}: {:?}",
                facts.problems
            );
            let target = facts
                .file(modal)
                .unwrap()
                .imports
                .iter()
                .find(|edge| edge.specifier.as_deref() == Some("./FilePreview"))
                .unwrap()
                .target
                .as_deref();
            assert_eq!(target, Some(expected_target));
            let diagnostics = config.rules[0].check(&facts).unwrap();
            assert_eq!(diagnostics.len(), usize::from(revision == "before"));
            if revision == "before" {
                assert_eq!(diagnostics[0].evidence.len(), 2);
                assert!(
                    diagnostics[0]
                        .evidence
                        .iter()
                        .any(|file| file == expected_target)
                );
                assert!(diagnostics[0].evidence.iter().any(|file| file == modal));
            }
        }
    }
}
