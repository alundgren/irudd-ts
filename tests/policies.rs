use archguard::{
    config::{Config, RuleConfig},
    facts::ProjectRule,
    project,
};
use serde_json::{Value, json};
use std::{fs, path::Path};
fn put(root: &Path, path: &str, source: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, source).unwrap();
}
fn config(value: Value) -> Config {
    let c: Config = serde_json::from_value(value).unwrap();
    c.validate().unwrap();
    c
}
fn check(root: &Path, value: Value) -> Vec<archguard::facts::Diagnostic> {
    let facts = project::analyze(root, &config(json!({"schemaVersion":1}))).unwrap();
    assert!(facts.problems.is_empty(), "{:?}", facts.problems);
    serde_json::from_value::<RuleConfig>(value)
        .unwrap()
        .check(&facts)
        .unwrap()
}
#[test]
fn platform_resolution_changes_the_actual_cycle() {
    let root = tempfile::tempdir().unwrap();
    put(root.path(), "modal.ts", "import './preview';");
    put(root.path(), "preview.ts", "export const view=1;");
    put(
        root.path(),
        "preview.ios.ts",
        "import type {View} from './modal';",
    );
    let r: RuleConfig = serde_json::from_value(json!({"id":"cycles","kind":"noCycles"})).unwrap();
    let c = config(json!({"schemaVersion":1}));
    assert!(
        r.check(&project::analyze(root.path(), &c).unwrap())
            .unwrap()
            .is_empty()
    );
    let c = config(json!({"schemaVersion":1,"extensions":[".ios.ts",".ts"]}));
    let facts = project::analyze(root.path(), &c).unwrap();
    assert!(facts.problems.is_empty());
    assert_eq!(
        facts.file("modal.ts").unwrap().imports[0].target.as_deref(),
        Some("preview.ios.ts")
    );
    assert_eq!(r.check(&facts).unwrap().len(), 1);
    put(root.path(), "types.ts", "export type View={};");
    put(
        root.path(),
        "preview.ios.ts",
        "import type {View} from './types';",
    );
    assert!(
        r.check(&project::analyze(root.path(), &c).unwrap())
            .unwrap()
            .is_empty()
    );
}
#[test]
fn runtime_reachability_has_dependency_evidence_and_respects_shadowing() {
    let root = tempfile::tempdir().unwrap();
    put(root.path(), "domain.ts", "import './helper';");
    put(
        root.path(),
        "helper.ts",
        "import * as E from 'effect/Effect'; E.runPromise(task);",
    );
    let policy = json!({"id":"runtime","kind":"forbiddenCall","files":["domain.ts"],"origins":["effect/Effect#runPromise"],"transitive":true,"includeTypes":false});
    let d = check(root.path(), policy.clone());
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].evidence[..2], ["domain.ts", "helper.ts"]);
    let mut direct = policy.clone();
    direct["transitive"] = json!(false);
    assert!(check(root.path(), direct).is_empty());
    put(
        root.path(),
        "helper.ts",
        "import * as E from 'effect/Effect'; function f(E:any){E.runPromise(task)}",
    );
    assert!(check(root.path(), policy).is_empty());
}
#[test]
fn service_and_package_contracts_have_failure_and_correction_controls() {
    let root = tempfile::tempdir().unwrap();
    put(
        root.path(),
        "a.ts",
        "import * as C from 'effect/Context'; export class A extends C.Service<A, {}>()('app/Service'){}",
    );
    put(
        root.path(),
        "b.ts",
        "import * as C from 'effect/Context'; export class B extends C.Service<B, {}>()('app/Service'){}",
    );
    let unique = json!({"id":"unique","kind":"uniqueServiceId"});
    assert_eq!(check(root.path(), unique.clone()).len(), 2);
    let layer = json!({"id":"layer","kind":"serviceLayer","files":["a.ts"]});
    assert_eq!(check(root.path(), layer.clone()).len(), 1);
    put(
        root.path(),
        "a.ts",
        "import * as C from 'effect/Context'; export class A extends C.Service<A, {}>()('app/A'){}; export const layer=A.layer;",
    );
    assert!(check(root.path(), unique).is_empty());
    assert!(check(root.path(), layer.clone()).is_empty());
    put(
        root.path(),
        "a.ts",
        "import * as C from 'effect/Context'; export class A extends C.Service<A, {}>()('app/A'){}; export type layer={};",
    );
    assert_eq!(check(root.path(), layer).len(), 1);
    put(
        root.path(),
        "package.json",
        r#"{"name":"app","dependencies":{"react":"*"}}"#,
    );
    let package = json!({"id":"deps","kind":"packageDependency","files":["package.json"],"specifiers":["react"]});
    assert_eq!(check(root.path(), package.clone()).len(), 1);
    put(
        root.path(),
        "package.json",
        r#"{"name":"app","dependencies":{}}"#,
    );
    assert!(check(root.path(), package).is_empty());
    let required = json!({"id":"file","kind":"requiredFile","files":["boundary.ts"]});
    assert_eq!(check(root.path(), required.clone()).len(), 1);
    put(root.path(), "boundary.ts", "export {};");
    assert!(check(root.path(), required).is_empty());
}
#[test]
fn public_entry_distinguishes_package_import_from_private_file() {
    let root = tempfile::tempdir().unwrap();
    put(
        root.path(),
        "packages/api/package.json",
        r#"{"name":"@app/api","exports":{".":"./src/index.ts"}}"#,
    );
    put(
        root.path(),
        "packages/api/src/index.ts",
        "export const api=1;",
    );
    put(root.path(), "main.ts", "import './packages/api/src/index';");
    let policy = json!({"id":"entry","kind":"publicEntry","files":["main.ts"],"targets":["packages/api/src/**"],"specifiers":["@app/api"]});
    assert_eq!(check(root.path(), policy.clone()).len(), 1);
    put(root.path(), "main.ts", "import '@app/api';");
    assert!(check(root.path(), policy).is_empty());
}
