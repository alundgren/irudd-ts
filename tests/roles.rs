use archguard::{config::Config, facts::ProjectRule, project, roles::RepositoryPolicy, rules};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

fn put(root: &Path, path: &str, source: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, source).unwrap();
}
fn policy() -> Config {
    serde_json::from_str(include_str!("../presets/t3code-structure.json")).unwrap()
}
fn check(root: &Path, config: &Config) -> Vec<archguard::facts::Diagnostic> {
    config.validate().unwrap();
    let facts = project::analyze(root, config).unwrap();
    assert!(facts.problems.is_empty(), "{:?}", facts.problems);
    rules::check(&facts, config).unwrap()
}
fn clean_fixture(root: &Path) {
    for (path, source) in [
        (
            "apps/server/src/persistence/Migrations/001_Events.ts",
            "export default 1;",
        ),
        (
            "apps/server/src/persistence/Migrations/002_Projects.ts",
            "export default 2;",
        ),
        (
            "apps/server/src/persistence/Migrations/001_Events.test.ts",
            "import migration from './001_Events.ts';",
        ),
        (
            "apps/server/src/persistence/Migrations.ts",
            "import a from './Migrations/001_Events.ts'; import b from './Migrations/002_Projects.ts'; export const entries=[a,b];",
        ),
        (
            "apps/server/src/persistence/Services/Projects.ts",
            "export class Projects {}",
        ),
        (
            "apps/server/src/persistence/Layers/Projects.ts",
            "import {Projects} from '../Services/Projects.ts'; export const live=Projects;",
        ),
        (
            "apps/server/src/persistence/Layers/Sqlite.ts",
            "export const sqlite=1;",
        ),
        (
            "apps/server/src/mcp/toolkits/preview/tools.ts",
            "export const standard=1; export const screenshot=2;",
        ),
        (
            "apps/server/src/mcp/toolkits/preview/handlers.ts",
            "import {standard,screenshot} from './tools.ts'; export const layers=[standard,screenshot];",
        ),
        (
            "apps/server/src/mcp/toolkits/preview/handlers.test.ts",
            "import './handlers.ts';",
        ),
        (
            "packages/contracts/src/rpc.ts",
            "export type Message={id:string};",
        ),
        (
            "apps/server/src/ws.ts",
            "import type {Message} from '../../../packages/contracts/src/rpc.ts';",
        ),
    ] {
        put(root, path, source);
    }
}

#[test]
fn t3_structural_simulation_has_failure_correction_and_controls() {
    let root = tempfile::tempdir().unwrap();
    let config = policy();
    clean_fixture(root.path());
    assert!(check(root.path(), &config).is_empty());
    let cases = [
        (
            "apps/server/src/persistence/Migrations/003_New.ts",
            "export default 3;",
            "migration-registry-import",
        ),
        (
            "apps/server/src/persistence/Services/Missing.ts",
            "export const service=1;",
            "service-layer-companion",
        ),
        (
            "apps/server/src/mcp/toolkits/new/tools.ts",
            "export const toolkit=1;",
            "tool-handler-companion",
        ),
        (
            "apps/server/src/persistence/Migrations/readme.ts",
            "export {};",
            "structure-classification",
        ),
    ];
    for (path, source, id) in cases {
        put(root.path(), path, source);
        let diagnostics = check(root.path(), &config);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].rule, id);
        fs::remove_file(root.path().join(path)).unwrap();
        assert!(check(root.path(), &config).is_empty());
    }
    let handler = "apps/server/src/mcp/toolkits/preview/handlers.ts";
    put(
        root.path(),
        handler,
        "import '../../../persistence/Migrations/001_Events.ts';",
    );
    assert_eq!(
        check(root.path(), &config)[0].rule,
        "handlers-no-migrations"
    );
    clean_fixture(root.path());
    assert!(check(root.path(), &config).is_empty());
    fs::remove_file(
        root.path()
            .join("apps/server/src/mcp/toolkits/preview/handlers.test.ts"),
    )
    .unwrap();
    assert_eq!(check(root.path(), &config)[0].rule, "tool-handler-test");
    clean_fixture(root.path());
    put(
        root.path(),
        "packages/contracts/src/rpc.ts",
        "import '../../../apps/server/src/ws.ts';",
    );
    assert_eq!(check(root.path(), &config)[0].rule, "contracts-no-host");
    clean_fixture(root.path());
    put(
        root.path(),
        "apps/server/src/persistence/Services/Projects.ts",
        "import type {live} from '../Layers/Projects.ts';",
    );
    assert_eq!(
        check(root.path(), &config)[0].rule,
        "services-no-live-layer"
    );
    clean_fixture(root.path());
    assert!(check(root.path(), &config).is_empty());
}

#[test]
fn registration_requires_static_value_edge_and_keeps_resolution_errors_visible() {
    let root = tempfile::tempdir().unwrap();
    let config = policy();
    clean_fixture(root.path());
    for registry in [
        "import type m from './Migrations/001_Events.ts'; import n from './Migrations/002_Projects.ts';",
        "import('./Migrations/001_Events.ts'); import n from './Migrations/002_Projects.ts';",
        "import n from './Migrations/002_Projects.ts';",
        "function unused() { require('./Migrations/001_Events.ts'); } import n from './Migrations/002_Projects.ts';",
        "export { default as m } from './Migrations/001_Events.ts'; import n from './Migrations/002_Projects.ts';",
    ] {
        put(
            root.path(),
            "apps/server/src/persistence/Migrations.ts",
            registry,
        );
        let diagnostics = check(root.path(), &config);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule, "migration-registry-import");
    }
    clean_fixture(root.path());
    put(
        root.path(),
        "apps/server/src/persistence/Migrations.ts",
        "import m = require('./Migrations/001_Events.ts'); import n from './Migrations/002_Projects.ts';",
    );
    assert!(check(root.path(), &config).is_empty());
    clean_fixture(root.path());
    put(
        root.path(),
        "apps/server/src/persistence/Migrations.ts",
        "import './Migrations/missing.ts';",
    );
    let config_path = root.path().join("archguard.json");
    fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_archguard"))
        .args(["check", "--root"])
        .arg(root.path())
        .arg("--config")
        .arg(config_path)
        .arg("--json")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["complete"], false);
    assert!(!report["problems"].as_array().unwrap().is_empty());
}

#[test]
fn roles_do_not_hide_unknown_or_overlapping_assignments() {
    let root = tempfile::tempdir().unwrap();
    put(root.path(), "main.ts", "export {}; ");
    put(root.path(), "unassigned.ts", "export {}; ");
    let config: Config = serde_json::from_value(json!({"schemaVersion":1})).unwrap();
    let facts = project::analyze(root.path(), &config).unwrap();
    let p: RepositoryPolicy = serde_json::from_value(json!({
        "roles":[{"id":"first","files":["main.ts"]},{"id":"second","files":["main.ts"]}],
        "rules":[{"id":"classify","kind":"classified","files":["*.ts"]}]
    }))
    .unwrap();
    let assignments = p.assignments(&facts).unwrap();
    assert_eq!(assignments["main.ts"], ["first", "second"]);
    assert!(assignments["unassigned.ts"].is_empty());
    assert_eq!(p.check(&facts).unwrap().len(), 2);
}

#[test]
fn role_dependencies_cross_unassigned_modules_and_respect_type_and_role_exclusions() {
    let root = tempfile::tempdir().unwrap();
    put(root.path(), "from/main.ts", "import '../middle.ts';");
    put(root.path(), "from/excluded.ts", "import '../to/main.ts';");
    put(
        root.path(),
        "middle.ts",
        "import type {Target} from './to/main.ts';",
    );
    put(root.path(), "to/main.ts", "export type Target={};");
    let mut value = json!({"schemaVersion":1,"repository":{
        "roles":[{"id":"source","files":["from/**"],"exclude":["from/excluded.ts"]},{"id":"target","files":["to/**"]}],
        "rules":[{"id":"boundary","kind":"forbiddenDependency","from":"source","to":"target","transitive":true}]
    }});
    let diagnostics = check(root.path(), &serde_json::from_value(value.clone()).unwrap());
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].evidence,
        ["from/main.ts", "middle.ts", "to/main.ts"]
    );
    value["repository"]["rules"][0]["includeTypes"] = json!(false);
    assert!(check(root.path(), &serde_json::from_value(value.clone()).unwrap()).is_empty());
    value["repository"]["rules"][0]["includeTypes"] = json!(true);
    value["repository"]["rules"][0]["transitive"] = json!(false);
    assert!(check(root.path(), &serde_json::from_value(value.clone()).unwrap()).is_empty());
    value["repository"]["rules"][0]["transitive"] = json!(true);
    value["repository"]["roles"][1]["exclude"] = json!(["to/main.ts"]);
    assert!(check(root.path(), &serde_json::from_value(value).unwrap()).is_empty());
}

#[test]
fn malformed_role_configuration_and_path_mapping_fail_closed() {
    for repository in [
        json!({"roles":[{"id":"x","files":[]}],"rules":[]}),
        json!({"roles":[{"id":"x","files":["["]}],"rules":[]}),
        json!({"roles":[],"rules":[{"id":"x","kind":"registryImport","role":"missing","registry":"a.ts"}]}),
        json!({"roles":[{"id":"x","files":["*.ts"]}],"rules":[{"id":"x","kind":"companion","role":"x","replace":["", "b"]}]}),
        json!({"roles":[{"id":"x","files":["*.ts"]}],"rules":[{"id":"x","kind":"registryImport","role":"x","registry":"../a.ts"}]}),
        json!({"roles":[],"rules":[{"id":"native","kind":"classified","files":["*.ts"]}]}),
    ] {
        let c: Config = serde_json::from_value(json!({"schemaVersion":1,"rules":[{"id":"native","kind":"noCycles"}],"repository":repository})).unwrap();
        assert!(c.validate().is_err());
    }
    assert!(serde_json::from_value::<Config>(json!({"schemaVersion":1,"repository":{"roles":[],"rules":[{"id":"x","kind":"classified","files":["*.ts"],"typo":true}]}})).is_err());
    let root = tempfile::tempdir().unwrap();
    put(root.path(), "main.ts", "export {}; ");
    let config: Config = serde_json::from_value(json!({"schemaVersion":1})).unwrap();
    let facts = project::analyze(root.path(), &config).unwrap();
    for replace in [["missing", "other"], ["main", "../other"]] {
        let p: RepositoryPolicy = serde_json::from_value(json!({"roles":[{"id":"x","files":["main.ts"]}],"rules":[{"id":"x","kind":"companion","role":"x","replace":replace}]})).unwrap();
        assert!(p.check(&facts).is_err());
    }
}
