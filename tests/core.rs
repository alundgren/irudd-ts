use archguard::{
    config::{Config, RuleConfig},
    facts::{ProjectRule, ResolutionStatus},
    project, typescript,
};
use serde_json::json;
use std::{fs, path::Path};
fn put(root: &Path, path: &str, source: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, source).unwrap();
}
fn config(value: serde_json::Value) -> Config {
    let c: Config = serde_json::from_value(value).unwrap();
    c.validate().unwrap();
    c
}
fn rule(value: serde_json::Value) -> RuleConfig {
    serde_json::from_value(value).unwrap()
}

#[test]
fn graph_tracks_workspace_exports_reexports_and_types() {
    let root = tempfile::tempdir().unwrap();
    put(
        root.path(),
        "packages/api/package.json",
        r#"{"name":"@app/api","exports":{".":{"types":"./src/index.ts","default":"./dist/index.js"},"./public":"./src/public.ts"}}"#,
    );
    put(
        root.path(),
        "packages/api/src/index.ts",
        "export { api } from './public.js';",
    );
    put(
        root.path(),
        "packages/api/src/public.ts",
        "export const api = 1;",
    );
    put(
        root.path(),
        "client/main.ts",
        "import type { api } from '@app/api'; import {api as a} from '@app/api/public';",
    );
    let facts = project::analyze(root.path(), &config(json!({"schemaVersion":1}))).unwrap();
    assert!(facts.problems.is_empty(), "{:?}", facts.problems);
    let imports = &facts.file("client/main.ts").unwrap().imports;
    assert!(imports[0].type_only);
    assert_eq!(imports[0].status, ResolutionStatus::Internal);
    assert_eq!(
        imports[0].target.as_deref(),
        Some("packages/api/src/index.ts")
    );
    let edge = &facts.file("packages/api/src/index.ts").unwrap().imports[0];
    assert_eq!(edge.kind, "reExport");
    assert_eq!(edge.target.as_deref(), Some("packages/api/src/public.ts"));
    put(root.path(), "client/main.ts", "import '@app/api/private';");
    let facts = project::analyze(root.path(), &config(json!({"schemaVersion":1}))).unwrap();
    assert_eq!(
        facts.file("client/main.ts").unwrap().imports[0].status,
        ResolutionStatus::Unresolved
    );
}
#[test]
fn inherited_aliases_and_missing_alias_targets_are_internal() {
    let root = tempfile::tempdir().unwrap();
    put(
        root.path(),
        "base.json",
        r#"{"compilerOptions":{"paths":{"logic/*":["./logic/*"]}}}"#,
    );
    put(root.path(), "tsconfig.json", r#"{"extends":"./base.json"}"#);
    put(root.path(), "logic/helper.ts", "export const helper=1;");
    put(root.path(), "main.ts", "import 'logic/helper';");
    let facts = project::analyze(root.path(), &config(json!({"schemaVersion":1}))).unwrap();
    assert!(facts.problems.is_empty(), "{:?}", facts.problems);
    assert_eq!(
        facts.file("main.ts").unwrap().imports[0].target.as_deref(),
        Some("logic/helper.ts")
    );
    put(root.path(), "main.ts", "import 'logic/missing';");
    let facts = project::analyze(root.path(), &config(json!({"schemaVersion":1}))).unwrap();
    assert_eq!(
        facts.file("main.ts").unwrap().imports[0].status,
        ResolutionStatus::Unresolved
    );
}
#[test]
fn exclusions_outside_root_dynamic_and_parse_errors_cannot_pass_clean() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("repo");
    fs::create_dir_all(&root).unwrap();
    put(parent.path(), "outside.ts", "export const x=1;");
    put(&root, "hidden.ts", "export const h=1;");
    put(
        &root,
        "main.ts",
        "import './hidden.ts'; import '../outside.ts'; import('./hidden.ts'); import(variable);",
    );
    let facts = project::analyze(
        &root,
        &config(json!({"schemaVersion":1,"exclude":["hidden.ts"]})),
    )
    .unwrap();
    let imports = &facts.file("main.ts").unwrap().imports;
    assert_eq!(imports.len(), 4);
    assert_eq!(imports[0].status, ResolutionStatus::Excluded);
    assert_eq!(imports[1].status, ResolutionStatus::OutsideRoot);
    assert_eq!(imports[2].status, ResolutionStatus::Excluded);
    assert_eq!(imports[3].status, ResolutionStatus::Unsupported);
    assert_eq!(facts.problems.len(), 4);
    put(&root, "broken.ts", "export const =");
    assert!(
        project::analyze(&root, &config(json!({"schemaVersion":1})))
            .unwrap()
            .problems
            .iter()
            .any(|p| p.file == "broken.ts")
    );
}
#[test]
fn provenance_handles_aliases_shadowing_strings_and_require() {
    let source = "import * as E from 'effect/Effect'; E.runPromise(1); function f(E: any) { E.runPromise(1); } const text='E.runPromise(1)'; function g(require: any) { require('fake'); } require('real');";
    let facts = typescript::parse("main.ts", source).unwrap();
    assert_eq!(
        facts
            .calls
            .iter()
            .filter(|c| c.origin.as_deref() == Some("effect/Effect#runPromise"))
            .count(),
        1
    );
    assert_eq!(
        facts.imports.iter().filter(|i| i.kind == "require").count(),
        1
    );
    assert!(
        facts
            .imports
            .iter()
            .any(|i| i.specifier.as_deref() == Some("real"))
    );
}
#[test]
fn service_namespace_resolves_definitions_and_barrels() {
    let root = tempfile::tempdir().unwrap();
    put(
        root.path(),
        "service.ts",
        "import * as C from 'effect/Context'; export class Foo extends C.Service<Foo, {}>()('app/Foo') {} export const layer=1; export const helper=1;",
    );
    put(
        root.path(),
        "barrel.ts",
        "export {Foo as Renamed,layer,helper} from './service.ts';",
    );
    put(
        root.path(),
        "consumer.ts",
        "import {Renamed,layer,helper} from './barrel.ts'; import type {Foo} from './service.ts';",
    );
    let facts = project::analyze(root.path(), &config(json!({"schemaVersion":1}))).unwrap();
    let r = rule(json!({"id":"namespace","kind":"serviceNamespace"}));
    assert_eq!(r.check(&facts).unwrap().len(), 2);
    put(
        root.path(),
        "consumer.ts",
        "import * as Foo from './service.ts'; import {helper} from './service.ts';",
    );
    let facts = project::analyze(root.path(), &config(json!({"schemaVersion":1}))).unwrap();
    assert!(r.check(&facts).unwrap().is_empty());
}
#[test]
fn transitive_rules_report_path_and_cycles_distinguish_types_and_dynamic_edges() {
    let root = tempfile::tempdir().unwrap();
    put(
        root.path(),
        "client/main.ts",
        "import '../shared/helper.ts';",
    );
    put(root.path(), "shared/helper.ts", "import '../server/db.ts';");
    put(root.path(), "server/db.ts", "export const db=1;");
    let facts = project::analyze(root.path(), &config(json!({"schemaVersion":1}))).unwrap();
    let r = rule(
        json!({"id":"client-server","kind":"forbiddenDependency","files":["client/**"],"targets":["server/**"],"transitive":true}),
    );
    let ds = r.check(&facts).unwrap();
    assert_eq!(ds.len(), 1);
    assert_eq!(
        ds[0].evidence,
        vec!["client/main.ts", "shared/helper.ts", "server/db.ts"]
    );
    put(
        root.path(),
        "server/db.ts",
        "import type {} from '../client/main.ts';",
    );
    let facts = project::analyze(root.path(), &config(json!({"schemaVersion":1}))).unwrap();
    assert_eq!(
        rule(json!({"id":"cycles","kind":"noCycles"}))
            .check(&facts)
            .unwrap()
            .len(),
        1
    );
    assert!(
        rule(json!({"id":"cycles","kind":"noCycles","includeTypes":false}))
            .check(&facts)
            .unwrap()
            .is_empty()
    );
    put(root.path(), "server/db.ts", "import('../client/main.ts');");
    let facts = project::analyze(root.path(), &config(json!({"schemaVersion":1}))).unwrap();
    assert!(
        rule(json!({"id":"cycles","kind":"noCycles"}))
            .check(&facts)
            .unwrap()
            .is_empty()
    );
}
#[test]
fn import_equals_cannot_bypass_dependency_policy() {
    let root = tempfile::tempdir().unwrap();
    put(
        root.path(),
        "client/main.ts",
        "import Hidden = require('../server/db.ts');",
    );
    put(root.path(), "server/db.ts", "export const db=1;");
    let facts = project::analyze(root.path(), &config(json!({"schemaVersion":1}))).unwrap();
    assert!(facts.problems.is_empty());
    assert_eq!(
        facts.file("client/main.ts").unwrap().imports[0].kind,
        "importEquals"
    );
    assert_eq!(rule(json!({"id":"boundary","kind":"forbiddenDependency","files":["client/**"],"targets":["server/**"]})).check(&facts).unwrap().len(),1);
}
#[test]
fn selected_null_export_is_blocked_and_namespace_exports_do_not_flatten() {
    let root = tempfile::tempdir().unwrap();
    put(
        root.path(),
        "packages/api/package.json",
        r#"{"name":"@app/api","exports":{".":{"types":null,"default":"./src/index.ts"}}}"#,
    );
    put(
        root.path(),
        "packages/api/src/index.ts",
        "export const api=1;",
    );
    put(root.path(), "main.ts", "import '@app/api';");
    let facts = project::analyze(root.path(), &config(json!({"schemaVersion":1}))).unwrap();
    assert_eq!(
        facts.file("main.ts").unwrap().imports[0].status,
        ResolutionStatus::Unresolved
    );
    put(
        root.path(),
        "main.ts",
        "export * as API from './packages/api/src/index.ts';",
    );
    let facts = project::analyze(root.path(), &config(json!({"schemaVersion":1}))).unwrap();
    assert_eq!(
        rule(json!({"id":"api","kind":"requiredExport","files":["main.ts"],"names":["api"]}))
            .check(&facts)
            .unwrap()
            .len(),
        1
    );
    put(
        root.path(),
        "main.ts",
        "export * from './packages/api/src/index.ts';",
    );
    let facts = project::analyze(root.path(), &config(json!({"schemaVersion":1}))).unwrap();
    assert!(
        rule(json!({"id":"api","kind":"requiredExport","files":["main.ts"],"names":["api"]}))
            .check(&facts)
            .unwrap()
            .is_empty()
    );
}
#[test]
fn local_export_aliases_keep_service_provenance() {
    let root = tempfile::tempdir().unwrap();
    put(
        root.path(),
        "service.ts",
        "import * as C from 'effect/Context'; class Foo extends C.Service<Foo, {}>()('app/Foo') {} export {Foo as Renamed};",
    );
    put(
        root.path(),
        "consumer.ts",
        "import {Renamed} from './service.ts';",
    );
    let facts = project::analyze(root.path(), &config(json!({"schemaVersion":1}))).unwrap();
    assert_eq!(
        rule(json!({"id":"namespace","kind":"serviceNamespace"}))
            .check(&facts)
            .unwrap()
            .len(),
        1
    );
    put(
        root.path(),
        "consumer.ts",
        "import * as Service from './service.ts';",
    );
    let facts = project::analyze(root.path(), &config(json!({"schemaVersion":1}))).unwrap();
    assert!(
        rule(json!({"id":"namespace","kind":"serviceNamespace"}))
            .check(&facts)
            .unwrap()
            .is_empty()
    );
}
#[test]
fn workspace_export_cannot_escape_the_package() {
    let root = tempfile::tempdir().unwrap();
    put(
        root.path(),
        "packages/api/package.json",
        r#"{"name":"@app/api","exports":{".":"./../private.ts"}}"#,
    );
    put(root.path(), "packages/private.ts", "export const x=1;");
    put(root.path(), "main.ts", "import '@app/api';");
    let facts = project::analyze(root.path(), &config(json!({"schemaVersion":1}))).unwrap();
    assert_eq!(
        facts.file("main.ts").unwrap().imports[0].status,
        ResolutionStatus::Unresolved
    );
    assert!(!facts.problems.is_empty());
}
#[test]
fn function_results_do_not_inherit_imported_method_identity() {
    let file = typescript::parse(
        "main.ts",
        "import {client} from './factory.ts'; client.run(); client().run();",
    )
    .unwrap();
    assert_eq!(
        file.calls
            .iter()
            .filter(|c| c.origin.as_deref() == Some("./factory.ts#client.run"))
            .count(),
        1
    );
    assert!(
        file.calls
            .iter()
            .any(|c| c.origin.as_deref() == Some("./factory.ts#client"))
    );
}
#[test]
fn exported_import_equals_and_array_fallbacks_preserve_dependencies() {
    let root = tempfile::tempdir().unwrap();
    put(
        root.path(),
        "packages/api/package.json",
        r#"{"name":"@app/api","exports":{".":[null,"../invalid.ts","./src/index.ts"]}}"#,
    );
    put(
        root.path(),
        "packages/api/src/index.ts",
        "export const api=1;",
    );
    put(
        root.path(),
        "main.ts",
        "export import Hidden = require('@app/api');",
    );
    let facts = project::analyze(root.path(), &config(json!({"schemaVersion":1}))).unwrap();
    assert!(facts.problems.is_empty(), "{:?}", facts.problems);
    assert_eq!(facts.file("main.ts").unwrap().imports.len(), 1);
    assert_eq!(
        facts.file("main.ts").unwrap().imports[0].status,
        ResolutionStatus::Internal
    );
}
#[test]
fn explicit_and_type_only_exports_do_not_become_service_values() {
    let root = tempfile::tempdir().unwrap();
    put(
        root.path(),
        "service.ts",
        "import * as C from 'effect/Context'; export class Foo extends C.Service<Foo, {}>()('app/Foo') {}",
    );
    put(
        root.path(),
        "barrel.ts",
        "export * from './service.ts'; export const Foo=1;",
    );
    put(
        root.path(),
        "consumer.ts",
        "import {Foo} from './barrel.ts';",
    );
    let r = rule(json!({"id":"namespace","kind":"serviceNamespace"}));
    let c = config(json!({"schemaVersion":1}));
    assert!(
        r.check(&project::analyze(root.path(), &c).unwrap())
            .unwrap()
            .is_empty()
    );
    put(
        root.path(),
        "barrel.ts",
        "export type * from './service.ts'; export const Foo=1;",
    );
    assert!(
        r.check(&project::analyze(root.path(), &c).unwrap())
            .unwrap()
            .is_empty()
    );
    put(
        root.path(),
        "barrel.ts",
        "export type {Foo} from './service.ts';",
    );
    assert!(
        r.check(&project::analyze(root.path(), &c).unwrap())
            .unwrap()
            .is_empty()
    );
}
#[test]
fn star_export_ambiguity_distinguishes_distinct_declarations_from_same_origin() {
    let root = tempfile::tempdir().unwrap();
    put(root.path(), "a.ts", "export const api=1;");
    put(root.path(), "b.ts", "export const api=2;");
    put(
        root.path(),
        "barrel.ts",
        "export * from './a.ts'; export * from './b.ts';",
    );
    let r = rule(json!({"id":"api","kind":"requiredExport","files":["barrel.ts"],"names":["api"]}));
    let c = config(json!({"schemaVersion":1}));
    let diagnostics = r
        .check(&project::analyze(root.path(), &c).unwrap())
        .unwrap();
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].message.contains("ambiguous"));
    put(root.path(), "b.ts", "export {api} from './a.ts';");
    assert!(
        r.check(&project::analyze(root.path(), &c).unwrap())
            .unwrap()
            .is_empty()
    );
}
