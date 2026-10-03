use archguard::{config::Config, project};
use std::{fs, path::Path};
fn put(root: &Path, path: &str, source: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, source).unwrap();
}
#[test]
fn published_profiles_validate() {
    for entry in fs::read_dir("examples/t3code/profiles").unwrap() {
        let path = entry.unwrap().path();
        let c: Config = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        c.validate().unwrap();
        assert!(!c.rules.is_empty(), "{}", path.display());
    }
}

#[test]
fn mobile_profiles_ignore_inactive_platform_cycles() {
    for (platform, inactive) in [("ios", "android"), ("android", "ios")] {
        let root = tempfile::tempdir().unwrap();
        put(
            root.path(),
            "apps/mobile/src/main.ts",
            "export const main=1;",
        );
        let a = format!("apps/mobile/src/a.{inactive}.ts");
        let b = format!("apps/mobile/src/b.{inactive}.ts");
        put(root.path(), &a, &format!("import './b.{inactive}.ts';"));
        put(root.path(), &b, &format!("import './a.{inactive}.ts';"));
        let c: Config = serde_json::from_str(
            &fs::read_to_string(format!(
                "examples/t3code/profiles/t3code-mobile-{platform}.json"
            ))
            .unwrap(),
        )
        .unwrap();
        let facts = project::analyze(root.path(), &c).unwrap();
        assert!(facts.problems.is_empty());
        assert_eq!(facts.files.len(), 1);
        assert!(archguard::rules::check(&facts, &c).unwrap().is_empty());
        put(
            root.path(),
            &format!("apps/mobile/src/a.{platform}.ts"),
            &format!("import './b.{platform}.ts';"),
        );
        put(
            root.path(),
            &format!("apps/mobile/src/b.{platform}.ts"),
            &format!("import './a.{platform}.ts';"),
        );
        let facts = project::analyze(root.path(), &c).unwrap();
        assert!(facts.problems.is_empty());
        assert_eq!(archguard::rules::check(&facts, &c).unwrap().len(), 1);
    }
}

#[test]
fn t3_presets_match_resolved_retired_sources_and_react_subpaths() {
    let root = tempfile::tempdir().unwrap();
    put(
        root.path(),
        "apps/server/src/orchestration/decider.ts",
        "export const decide=1;",
    );
    put(
        root.path(),
        "tsconfig.json",
        r#"{"compilerOptions":{"paths":{"@retired":["./apps/server/src/orchestration/decider.ts"]}}}"#,
    );
    let c: Config = serde_json::from_str(
        &fs::read_to_string("examples/t3code/profiles/t3code-legacy-boundary.json").unwrap(),
    )
    .unwrap();
    for spelling in [
        "./orchestration/decider.ts",
        "./orchestration/decider.js",
        "./orchestration/decider",
        "@retired",
    ] {
        put(
            root.path(),
            "apps/server/src/main.ts",
            &format!("import '{spelling}';"),
        );
        let facts = project::analyze(root.path(), &c).unwrap();
        assert!(facts.problems.is_empty(), "{:?}", facts.problems);
        let diagnostics = archguard::rules::check(&facts, &c).unwrap();
        assert_eq!(diagnostics.len(), 1, "{spelling}");
        assert_eq!(diagnostics[0].rule, "retired-v1-decider");
    }
    put(
        root.path(),
        "apps/server/src/helper.ts",
        "export const helper=1;",
    );
    put(root.path(), "apps/server/src/main.ts", "import './helper';");
    assert!(
        archguard::rules::check(&project::analyze(root.path(), &c).unwrap(), &c)
            .unwrap()
            .is_empty()
    );
    let c: Config =
        serde_json::from_str(&fs::read_to_string("examples/t3code/profiles/t3code.json").unwrap())
            .unwrap();
    put(
        root.path(),
        "packages/contracts/src/index.ts",
        "import {jsx} from 'react/jsx-runtime';",
    );
    let facts = project::analyze(root.path(), &c).unwrap();
    assert!(facts.problems.is_empty());
    assert_eq!(archguard::rules::check(&facts, &c).unwrap().len(), 1);
    put(
        root.path(),
        "packages/contracts/src/index.ts",
        "import type {JSX} from 'react/jsx-runtime';",
    );
    assert!(
        archguard::rules::check(&project::analyze(root.path(), &c).unwrap(), &c)
            .unwrap()
            .is_empty()
    );
}
