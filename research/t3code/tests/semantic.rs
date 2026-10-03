use archguard::semantic::*;
use serde_json::json;
use std::{fs, path::Path};
#[path = "../../../tests/support/semantic.rs"]
mod support;
use support::{config, graph};

#[test]
fn merged_pr_11304_reduced_before_fixed_and_partial_correction() {
    let root = tempfile::tempdir().unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("research/t3code/semantic/fixtures");
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("research/t3code/semantic/node_modules"),
        root.path().join("node_modules"),
    )
    .unwrap();
    let config = config(
        root.path(),
        json!([{"id":"history","tsconfig":"tsconfig.json","files":["main.ts"]}]),
    );
    let before = fs::read_to_string(fixture.join("before.ts")).unwrap();
    let fixed = fs::read_to_string(fixture.join("fixed.ts")).unwrap();
    for (source, clean) in [(&before, false), (&fixed, true)] {
        fs::write(root.path().join("main.ts"), source).unwrap();
        let facts = analyze(&graph(root.path()), &config, root.path()).unwrap();
        assert_eq!(facts.complete, clean, "{:?}", facts.problems());
        let findings = check(&facts, &config).unwrap();
        assert_eq!(findings.is_empty(), clean);
        if clean {
            assert!(
                facts
                    .file("history", "main.ts")
                    .unwrap()
                    .properties
                    .iter()
                    .any(|p| p.member == "webSocketTicket"
                        && p.status == MemberStatus::Present
                        && p.symbol.as_ref().is_some_and(|s| s.declarations.is_empty()))
            );
        } else {
            assert!(facts.contexts[0].diagnostics.iter().any(|d| d.code == 2339));
            assert!(
                facts
                    .file("history", "main.ts")
                    .unwrap()
                    .properties
                    .iter()
                    .any(|p| p.receiver.state == ReceiverState::Error)
            );
        }
    }
    let partial = before.replace(
        "remoteAuthorization,\n",
        "remoteAuthorization,\n      group: \"auth\",\n",
    );
    fs::write(root.path().join("main.ts"), partial).unwrap();
    let facts = analyze(&graph(root.path()), &config, root.path()).unwrap();
    assert!(!facts.complete);
    assert!(!check(&facts, &config).unwrap().is_empty());
}
