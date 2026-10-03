use archguard::{cache::CachedAnalysis, config::Config, facts::ResolutionStatus, project, rules};
use std::{fs, path::Path};

fn config() -> Config {
    serde_json::from_str(r#"{"schemaVersion":1}"#).unwrap()
}

fn write(root: &Path, path: &str, source: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, source).unwrap();
}

fn equivalent(root: &Path, config: &Config, cache: &Path) -> CachedAnalysis {
    let cached = project::analyze_cached(root, config, cache).unwrap();
    let fresh = project::analyze(root, config).unwrap();
    assert_eq!(
        serde_json::to_value(&cached.project).unwrap(),
        serde_json::to_value(&fresh).unwrap()
    );
    assert_eq!(
        serde_json::to_value(rules::check(&cached.project, config).unwrap()).unwrap(),
        serde_json::to_value(rules::check(&fresh, config).unwrap()).unwrap()
    );
    cached
}

fn target(analysis: &CachedAnalysis, source: &str) -> Option<String> {
    analysis.project.file(source).unwrap().imports[0]
        .target
        .clone()
}

#[test]
fn unchanged_graph_reuses_edges_but_source_bytes_override_timestamps() {
    let root = tempfile::tempdir().unwrap();
    let cache = root.path().join("cache.json");
    let config = config();
    write(root.path(), "a.ts", "import './b.ts';");
    write(root.path(), "b.ts", "export const b = 1;");
    write(root.path(), "c.ts", "export const c = 1;");
    let first = equivalent(root.path(), &config, &cache);
    assert!(first.cache.published);
    assert_eq!(first.cache.resolved_edges, 1);
    let unchanged = equivalent(root.path(), &config, &cache);
    assert_eq!(unchanged.cache.parsed_files, 0);
    assert_eq!(unchanged.cache.resolved_edges, 0);
    assert_eq!(unchanged.cache.reused_edges, 1);
    let source = root.path().join("a.ts");
    let modified = fs::metadata(&source).unwrap().modified().unwrap();
    fs::write(&source, "import './c.ts';").unwrap();
    fs::File::options()
        .write(true)
        .open(source)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(modified))
        .unwrap();
    let edited = equivalent(root.path(), &config, &cache);
    assert_eq!(target(&edited, "a.ts").as_deref(), Some("c.ts"));
    assert_eq!(edited.cache.parsed_files, 1);
    assert_eq!(edited.cache.resolved_edges, 1);
    write(root.path(), "b.ts", "export const b = 2;");
    let control = equivalent(root.path(), &config, &cache);
    assert_eq!(control.cache.reused_edges, 1);
}

#[test]
fn negative_lookups_include_unselected_and_ignored_targets() {
    let root = tempfile::tempdir().unwrap();
    let cache = root.path().join("cache.json");
    let mut config = config();
    config.include = vec!["a.ts".into(), "b.ts".into()];
    config.extensions = vec![".native.ts".into(), ".ts".into()];
    write(root.path(), "a.ts", "import './b';");
    write(root.path(), "b.ts", "export {};");
    equivalent(root.path(), &config, &cache);
    assert_eq!(
        equivalent(root.path(), &config, &cache).cache.reused_edges,
        1
    );
    write(root.path(), ".gitignore", "*.native.ts\n");
    write(root.path(), "b.native.ts", "export {};");
    let added = equivalent(root.path(), &config, &cache);
    assert_eq!(added.project.files.len(), 2);
    assert_eq!(
        added.project.file("a.ts").unwrap().imports[0].status,
        ResolutionStatus::Excluded
    );
    assert_eq!(added.cache.reused_edges, 0);
    assert!(!added.cache.published);
    fs::remove_file(root.path().join("b.native.ts")).unwrap();
    assert!(
        equivalent(root.path(), &config, &cache)
            .project
            .problems
            .is_empty()
    );
}

#[test]
fn inherited_config_outside_root_and_new_nearest_config_invalidate_edges() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("project");
    fs::create_dir(&root).unwrap();
    let cache = parent.path().join("cache.json");
    let config = config();
    write(&root, "use.ts", "import '#value';");
    write(&root, "a.ts", "export {};");
    write(&root, "b.ts", "export {};");
    write(
        parent.path(),
        "tsconfig.json",
        r#"{"extends":"./base.json","include":["project/**/*.ts"]}"#,
    );
    write(
        parent.path(),
        "base.json",
        r##"{"compilerOptions":{"paths":{"#value":["./project/a.ts"]}}}"##,
    );
    assert_eq!(
        target(&equivalent(&root, &config, &cache), "use.ts").as_deref(),
        Some("a.ts")
    );
    assert_eq!(equivalent(&root, &config, &cache).cache.reused_edges, 1);
    write(
        parent.path(),
        "base.json",
        r##"{"compilerOptions":{"paths":{"#value":["./project/b.ts"]}}}"##,
    );
    let changed = equivalent(&root, &config, &cache);
    assert_eq!(target(&changed, "use.ts").as_deref(), Some("b.ts"));
    assert_eq!(changed.cache.reused_edges, 0);
    write(
        &root,
        "tsconfig.json",
        r##"{"include":["**/*.ts"],"compilerOptions":{"paths":{"#value":["./a.ts"]}}}"##,
    );
    let nearest = equivalent(&root, &config, &cache);
    assert_eq!(target(&nearest, "use.ts").as_deref(), Some("a.ts"));
    assert_eq!(nearest.cache.reused_edges, 0);
    write(&root, "tsconfig.json", "{");
    let broken = equivalent(&root, &config, &cache);
    assert!(!broken.project.problems.is_empty());
    assert!(!broken.cache.published);
    fs::remove_file(root.join("tsconfig.json")).unwrap();
    assert!(
        equivalent(&root, &config, &cache)
            .project
            .problems
            .is_empty()
    );
}

#[test]
fn workspace_and_installed_manifests_and_missing_packages_are_inputs() {
    let root = tempfile::tempdir().unwrap();
    let cache = root.path().join("cache.json");
    let mut config = config();
    write(
        root.path(),
        "use.ts",
        "import '@local/lib'; import 'installed';",
    );
    write(root.path(), "packages/lib/a.ts", "export {};");
    write(root.path(), "packages/lib/b.ts", "export {};");
    write(
        root.path(),
        "packages/lib/package.json",
        r#"{"name":"@local/lib","exports":"./a.ts"}"#,
    );
    let initial = equivalent(root.path(), &config, &cache);
    assert!(initial.project.problems.is_empty());
    assert_eq!(
        equivalent(root.path(), &config, &cache).cache.reused_edges,
        2
    );
    write(
        root.path(),
        "packages/lib/package.json",
        r#"{"name":"@local/lib","exports":"./b.ts"}"#,
    );
    let workspace = equivalent(root.path(), &config, &cache);
    assert_eq!(
        target(&workspace, "use.ts").as_deref(),
        Some("packages/lib/b.ts")
    );
    assert_eq!(workspace.cache.reused_edges, 0);
    write(
        root.path(),
        "node_modules/installed/package.json",
        r#"{"name":"installed","exports":"./index.ts"}"#,
    );
    write(root.path(), "node_modules/installed/index.ts", "export {};");
    let installed = equivalent(root.path(), &config, &cache);
    assert_eq!(installed.cache.reused_edges, 0);
    assert_eq!(
        installed.project.file("use.ts").unwrap().imports[1].detail,
        None
    );
    config.require_external_resolution = true;
    assert!(
        equivalent(root.path(), &config, &cache)
            .project
            .problems
            .is_empty()
    );
    write(
        root.path(),
        "node_modules/installed/package.json",
        r#"{"name":"installed","exports":"./absent.ts"}"#,
    );
    let missing = equivalent(root.path(), &config, &cache);
    assert!(!missing.project.problems.is_empty());
    assert!(!missing.cache.published);
    write(
        root.path(),
        "node_modules/installed/absent.ts",
        "export {};",
    );
    assert!(
        equivalent(root.path(), &config, &cache)
            .project
            .problems
            .is_empty()
    );
    write(
        root.path(),
        "packages/lib/package.json",
        r#"{"name":"@local/other","exports":"./b.ts"}"#,
    );
    assert!(
        !equivalent(root.path(), &config, &cache)
            .project
            .problems
            .is_empty()
    );
    write(
        root.path(),
        "packages/lib/package.json",
        r#"{"name":"@local/lib","exports":"./b.ts"}"#,
    );
    assert!(
        equivalent(root.path(), &config, &cache)
            .project
            .problems
            .is_empty()
    );
}

#[test]
fn discovery_parse_failure_correction_and_profile_changes_match_fresh() {
    let root = tempfile::tempdir().unwrap();
    let cache = root.path().join("cache.json");
    let mut config = config();
    write(root.path(), "a.ts", "import './b';");
    write(root.path(), "b.ts", "export {};");
    equivalent(root.path(), &config, &cache);
    write(root.path(), "b.ts", "export const =");
    let broken = equivalent(root.path(), &config, &cache);
    assert!(!broken.project.problems.is_empty());
    assert!(!broken.cache.published);
    write(root.path(), "b.ts", "export {};");
    assert!(
        equivalent(root.path(), &config, &cache)
            .project
            .problems
            .is_empty()
    );
    fs::rename(root.path().join("b.ts"), root.path().join("c.ts")).unwrap();
    assert!(
        !equivalent(root.path(), &config, &cache)
            .project
            .problems
            .is_empty()
    );
    write(root.path(), "a.ts", "import './c';");
    assert!(
        equivalent(root.path(), &config, &cache)
            .project
            .problems
            .is_empty()
    );
    config.exclude.push("c.ts".into());
    assert!(
        !equivalent(root.path(), &config, &cache)
            .project
            .problems
            .is_empty()
    );
    config.exclude.clear();
    write(root.path(), "c.js", "export {};");
    config.extensions = vec![".js".into(), ".ts".into()];
    assert_eq!(
        target(&equivalent(root.path(), &config, &cache), "a.ts").as_deref(),
        Some("c.js")
    );
    config.extensions.reverse();
    assert_eq!(
        target(&equivalent(root.path(), &config, &cache), "a.ts").as_deref(),
        Some("c.ts")
    );
}

#[test]
fn corrupt_entries_fail_once_recover_and_incompatible_versions_are_misses() {
    let root = tempfile::tempdir().unwrap();
    let cache = root.path().join("cache.json");
    let config = config();
    write(root.path(), "a.ts", "export {};");
    equivalent(root.path(), &config, &cache);
    for corruption in [b"{".as_slice(), b"{}".as_slice()] {
        fs::write(&cache, corruption).unwrap();
        let broken = project::analyze_cached(root.path(), &config, &cache).unwrap();
        assert!(!broken.project.problems.is_empty());
        assert_eq!(broken.cache.reused_files, 0);
        assert!(broken.cache.published);
        assert!(
            equivalent(root.path(), &config, &cache)
                .project
                .problems
                .is_empty()
        );
    }
    let mut envelope: serde_json::Value =
        serde_json::from_slice(&fs::read(&cache).unwrap()).unwrap();
    envelope["snapshot"]["sources"]["a.ts"]["syntax"]["bytes"] = 999.into();
    fs::write(&cache, serde_json::to_vec(&envelope).unwrap()).unwrap();
    assert!(
        project::analyze_cached(root.path(), &config, &cache)
            .unwrap()
            .project
            .problems[0]
            .message
            .contains("checksum")
    );
    let mut envelope: serde_json::Value =
        serde_json::from_slice(&fs::read(&cache).unwrap()).unwrap();
    envelope["version"] = 999.into();
    fs::write(&cache, serde_json::to_vec(&envelope).unwrap()).unwrap();
    let incompatible = equivalent(root.path(), &config, &cache);
    assert!(incompatible.project.problems.is_empty());
    assert_eq!(incompatible.cache.reused_files, 0);
    fs::remove_file(&cache).unwrap();
    fs::create_dir(&cache).unwrap();
    assert!(
        !project::analyze_cached(root.path(), &config, &cache)
            .unwrap()
            .project
            .problems
            .is_empty()
    );
    fs::remove_dir(&cache).unwrap();
    assert!(
        equivalent(root.path(), &config, &cache)
            .project
            .problems
            .is_empty()
    );
}

#[test]
fn cache_destination_cannot_replace_selected_source_or_manifest() {
    let root = tempfile::tempdir().unwrap();
    write(root.path(), "a.ts", "export {};");
    write(root.path(), "package.json", "{}");
    for name in ["a.ts", "package.json", "new.ts"] {
        assert!(project::analyze_cached(root.path(), &config(), &root.path().join(name)).is_err());
    }
    assert_eq!(
        fs::read_to_string(root.path().join("a.ts")).unwrap(),
        "export {};"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("package.json")).unwrap(),
        "{}"
    );
    assert!(!root.path().join("new.ts").exists());
}

#[test]
fn cache_destination_cannot_replace_inherited_resolution_input() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("project");
    fs::create_dir(&root).unwrap();
    write(&root, "a.ts", "import './b.ts';");
    write(&root, "b.ts", "export {};");
    let source = r#"{"include":["project/**/*.ts"]}"#;
    write(parent.path(), "tsconfig.json", source);
    let input = parent.path().join("tsconfig.json");
    let cached = project::analyze_cached(&root, &config(), &input).unwrap();
    assert!(!cached.project.problems.is_empty());
    assert!(!cached.cache.published);
    assert_eq!(fs::read_to_string(&input).unwrap(), source);
}

#[cfg(unix)]
#[test]
fn installed_package_link_repointing_invalidates_resolution() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("project");
    fs::create_dir(&root).unwrap();
    let cache = parent.path().join("cache.json");
    let mut config = config();
    config.require_external_resolution = true;
    write(&root, "use.ts", "import 'installed';");
    for name in ["store-a", "store-b"] {
        write(
            parent.path(),
            &format!("node_modules/{name}/package.json"),
            r#"{"name":"installed","exports":"./index.ts"}"#,
        );
        write(
            parent.path(),
            &format!("node_modules/{name}/index.ts"),
            "export {};",
        );
    }
    fs::create_dir(root.join("node_modules")).unwrap();
    let link = root.join("node_modules/installed");
    std::os::unix::fs::symlink(parent.path().join("node_modules/store-a"), &link).unwrap();
    assert!(
        equivalent(&root, &config, &cache)
            .project
            .problems
            .is_empty()
    );
    assert_eq!(equivalent(&root, &config, &cache).cache.reused_edges, 1);
    fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink(parent.path().join("node_modules/store-b"), &link).unwrap();
    let repointed = equivalent(&root, &config, &cache);
    assert_eq!(repointed.cache.reused_edges, 0);
    fs::remove_file(parent.path().join("node_modules/store-b/index.ts")).unwrap();
    assert!(
        !equivalent(&root, &config, &cache)
            .project
            .problems
            .is_empty()
    );
}

#[test]
fn rust_candidates_and_root_identity_are_validated() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("one");
    fs::create_dir(&root).unwrap();
    let cache = parent.path().join("cache.json");
    let mut config = config();
    config.include = vec!["**/*.rs".into()];
    write(&root, "src/main.rs", "use crate::worker::run; fn main() {}");
    write(&root, "src/worker.rs", "pub fn run() {}");
    equivalent(&root, &config, &cache);
    assert_eq!(equivalent(&root, &config, &cache).cache.reused_edges, 1);
    fs::remove_file(root.join("src/worker.rs")).unwrap();
    assert!(
        !equivalent(&root, &config, &cache)
            .project
            .problems
            .is_empty()
    );
    write(&root, "src/worker/mod.rs", "pub fn run() {}");
    assert_eq!(
        target(&equivalent(&root, &config, &cache), "src/main.rs").as_deref(),
        Some("src/worker/mod.rs")
    );
    let moved = parent.path().join("two");
    fs::rename(root, &moved).unwrap();
    let relocated = equivalent(&moved, &config, &cache);
    assert_eq!(relocated.cache.reused_files, 0);
    assert!(relocated.project.problems.is_empty());
}

#[cfg(unix)]
#[test]
fn cli_reused_graph_still_runs_plugins_and_reports_corruption() {
    let root = tempfile::tempdir().unwrap();
    let cache = root.path().join("cache.json");
    write(root.path(), "a.ts", "import './b.ts';");
    write(root.path(), "b.ts", "export {};");
    let config = serde_json::json!({"schemaVersion":1,"include":["a.ts","b.ts"],"plugins":[{
        "name":"observe","command":["node","-e",
        "const fs=require('fs');fs.appendFileSync('calls','x');process.stdin.resume();process.stdin.on('end',()=>{if(fs.existsSync('fail'))process.exit(4);else console.log(JSON.stringify({schemaVersion:1,diagnostics:[]}));});"]}]});
    write(root.path(), "archguard.json", &config.to_string());
    let invoke = || {
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_archguard"))
            .args(["check", "--json", "--root"])
            .arg(root.path())
            .arg("--config")
            .arg(root.path().join("archguard.json"))
            .arg("--cache")
            .arg(&cache)
            .output()
            .unwrap();
        assert!(result.stderr.is_empty());
        (
            result.status.code().unwrap(),
            serde_json::from_slice::<serde_json::Value>(&result.stdout).unwrap(),
        )
    };
    assert_eq!(invoke().0, 0);
    let unchanged = invoke();
    assert_eq!(unchanged.0, 0);
    assert_eq!(unchanged.1["cache"]["reusedEdges"], 1);
    write(root.path(), "fail", "");
    let failed = invoke();
    assert_eq!(failed.0, 2);
    assert_eq!(failed.1["complete"], false);
    assert_eq!(failed.1["cache"]["reusedEdges"], 1);
    assert_eq!(
        fs::read_to_string(root.path().join("calls")).unwrap(),
        "xxx"
    );
    fs::remove_file(root.path().join("fail")).unwrap();
    fs::write(&cache, "{").unwrap();
    let corrupt = invoke();
    assert_eq!(corrupt.0, 2);
    assert_eq!(corrupt.1["complete"], false);
    assert_eq!(corrupt.1["cache"]["reusedEdges"], 0);
    assert_eq!(invoke().0, 0);
}
