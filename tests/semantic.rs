use archguard::{config::Config, project, semantic::*};
use serde_json::json;
use std::{fs, path::Path};
fn config(root: &Path, contexts: serde_json::Value) -> SemanticConfig {
    let provider = Path::new(env!("CARGO_MANIFEST_DIR")).join("providers/typescript7/provider.mjs");
    fs::write(root.join("tsconfig.json"),r#"{"compilerOptions":{"strict":true,"noEmit":true,"skipLibCheck":true,"target":"ESNext","module":"NodeNext","moduleResolution":"NodeNext"},"include":["*.ts"]}"#).unwrap();
    serde_json::from_value(json!({"schemaVersion":1,"provider":{"name":"typescript7","command":["node",provider],"timeoutMs":30000},"contexts":contexts,"rules":[{"id":"missing-member","kind":"missingMember","files":["**"]}]})).unwrap()
}
fn graph(root: &Path) -> archguard::facts::ProjectFacts {
    project::analyze(
        root,
        &serde_json::from_value::<Config>(json!({"schemaVersion":1,"include":["*.ts"]})).unwrap(),
    )
    .unwrap()
}
fn clean_response(request: &SemanticRequest) -> SemanticFacts {
    serde_json::from_value(json!({"schemaVersion":1,"root":request.root,"backend":request.backend,"complete":true,"contexts":request.contexts.iter().map(|c|json!({"id":c.id,"tsconfig":c.tsconfig,"configSha256":c.config_sha256,"complete":true,"problems":[],"diagnostics":[],"files":c.files.iter().map(|f|json!({"path":f.path,"bytes":f.bytes,"sha256":f.sha256,"available":true,"properties":f.sites.iter().map(|s|json!({"offset":s.offset,"member":s.member,"receiver":{"state":"known","display":"{value:number}"},"status":"present","symbol":{"name":s.member,"declarations":[]},"detail":null})).collect::<Vec<_>>()})).collect::<Vec<_>>()})).collect::<Vec<_>>()})).unwrap()
}
#[test]
fn independently_enumerates_public_sites_and_rejects_omissions() {
    let root = tempfile::tempdir().unwrap();
    let source = "const emoji='😀'; export const x={value:1}; x.value; x?.value; x['value']; class C { #private=1; f(){ return this.#private; } }";
    fs::write(root.path().join("main.ts"), source).unwrap();
    let contexts = json!([{"id":"a","tsconfig":"tsconfig.json","files":["*.ts"]},{"id":"b","tsconfig":"tsconfig.json","files":["*.ts"]}]);
    let config = config(root.path(), contexts);
    let request = request(&graph(root.path()), &config).unwrap();
    assert_eq!(request.contexts.len(), 2);
    assert_eq!(request.contexts[0].files[0].sites.len(), 2);
    let byte_offset = source.find("x.value").unwrap() + 2;
    assert_eq!(request.contexts[0].files[0].sites[0].offset, byte_offset);
    let response = clean_response(&request);
    validate(&request, &response).unwrap();
    for case in 0..8 {
        let mut bad = response.clone();
        match case {
            0 => {
                bad.contexts.pop();
            }
            1 => {
                bad.contexts[0].files.clear();
            }
            2 => {
                bad.contexts[0].files[0].properties.pop();
            }
            3 => {
                let duplicate = bad.contexts[0].files[0].properties[0].clone();
                bad.contexts[0].files[0].properties.push(duplicate);
            }
            4 => bad.contexts[0].files[0].properties[0].offset += 1,
            5 => bad.contexts[0].files[0].properties[0].member = "other".into(),
            6 => bad.contexts[0].files[0].properties[0].receiver.state = ReceiverState::Any,
            _ => {
                bad.contexts[0].problems.push("failed".into());
            }
        }
        assert!(
            validate(&request, &bad).is_err(),
            "accepted invalid case {case}"
        );
    }
    let mut value = serde_json::to_value(&response).unwrap();
    value["unexpected"] = json!(true);
    assert!(serde_json::from_value::<SemanticFacts>(value).is_err());
    fs::write(
        root.path().join("main.ts"),
        source.replace("value:1", "value:2"),
    )
    .unwrap();
    assert!(validate(&request, &response).is_err());
}
#[test]
fn native_jsx_member_tags_have_independent_sites_and_facts() {
    let root = tempfile::tempdir().unwrap();
    let config = config(
        root.path(),
        json!([{"id":"jsx","tsconfig":"tsconfig.json","files":["*.tsx"]}]),
    );
    fs::write(root.path().join("tsconfig.json"), r#"{"compilerOptions":{"strict":true,"noEmit":true,"skipLibCheck":true,"jsx":"preserve","target":"ESNext","module":"NodeNext","moduleResolution":"NodeNext"},"files":["main.tsx"]}"#).unwrap();
    let analyze_source = |source: &str| {
        fs::write(root.path().join("main.tsx"), source).unwrap();
        let project = project::analyze(
            root.path(),
            &serde_json::from_value::<Config>(json!({"schemaVersion":1,"include":["*.tsx"]}))
                .unwrap(),
        )
        .unwrap();
        analyze(&project, &config, root.path()).unwrap()
    };
    let before = "const View={Child:()=>null}; const tag=<View.Missing/>;";
    let facts = analyze_source(before);
    let properties = &facts.file("jsx", "main.tsx").unwrap().properties;
    assert_eq!(properties.len(), 1);
    assert_eq!(properties[0].offset, before.find("Missing").unwrap());
    assert_eq!(properties[0].receiver.state, ReceiverState::Known);
    assert_eq!(properties[0].status, MemberStatus::Missing);
    assert!(!facts.complete);
    let fixed = analyze_source("const View={Child:()=>null}; const tag=<View.Child/>;");
    assert!(fixed.complete);
    assert_eq!(
        fixed.file("jsx", "main.tsx").unwrap().properties[0].status,
        MemberStatus::Present
    );
    let nested = inventory(
        "main.tsx",
        "const tag=<View.Nested.Child></View.Nested.Child>;",
    )
    .unwrap();
    assert_eq!(
        nested
            .iter()
            .map(|site| site.member.as_str())
            .collect::<Vec<_>>(),
        vec!["Nested", "Child", "Nested", "Child"]
    );
    let nested_facts = analyze_source(
        "const emoji='😀'; const View={Nested:{Child:()=>null}}; const tag=<View.Nested.Child></View.Nested.Child>;",
    );
    assert!(nested_facts.complete);
    let nested_properties = &nested_facts.file("jsx", "main.tsx").unwrap().properties;
    assert_eq!(nested_properties.len(), 4);
    assert!(
        nested_properties
            .iter()
            .all(|property| property.status == MemberStatus::Present)
    );
}
#[test]
fn native_provider_unicode_multicontext_and_unavailable_controls() {
    let root = tempfile::tempdir().unwrap();
    let source = "export const emoji='😀'; const x={value:1}; x.value; x.missing; const unrelated={auth:{webSocketTicket:1}}; unrelated.auth.webSocketTicket; declare const untyped:any; untyped.auth; declare const unknownReceiver:unknown; unknownReceiver.auth;";
    fs::write(root.path().join("main.ts"), source).unwrap();
    let config = config(
        root.path(),
        json!([{"id":"a","tsconfig":"tsconfig.json","files":["*.ts"]},{"id":"b","tsconfig":"tsconfig.json","files":["*.ts"]}]),
    );
    let facts = analyze(&graph(root.path()), &config, root.path()).unwrap();
    assert!(!facts.complete);
    assert_eq!(facts.contexts.len(), 2);
    let properties = &facts.file("a", "main.ts").unwrap().properties;
    let missing = properties.iter().find(|p| p.member == "missing").unwrap();
    assert_eq!(missing.offset, source.find("x.missing").unwrap() + 2);
    assert_eq!(missing.receiver.state, ReceiverState::Known);
    assert_eq!(missing.status, MemberStatus::Missing);
    assert!(
        properties
            .iter()
            .any(|p| p.receiver.state == ReceiverState::Any)
    );
    assert!(
        properties
            .iter()
            .any(|p| p.receiver.state == ReceiverState::Unknown)
    );
    let findings = check(&facts, &config).unwrap();
    assert_eq!(findings.len(), 2);
    assert!(findings.iter().all(|f| f.message.contains("missing")));
    let sdk = Path::new(env!("CARGO_MANIFEST_DIR")).join("sdk/semantic.ts");
    let script = format!(
        "import {{readSemanticFacts,missingMemberDiagnostics}} from {sdk:?}; process.stdout.write(JSON.stringify(missingMemberDiagnostics(readSemanticFacts())));"
    );
    let mut child = std::process::Command::new("node")
        .args(["--input-type=module", "-e", &script])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&facts).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let ts: Vec<archguard::facts::Diagnostic> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(findings, ts);
}
#[test]
fn native_clean_and_context_membership_failures() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("main.ts"),
        "export const x={value:1}; x.value;",
    )
    .unwrap();
    let config = config(
        root.path(),
        json!([{"id":"a","tsconfig":"tsconfig.json","files":["*.ts"]}]),
    );
    let project = graph(root.path());
    let facts = analyze(&project, &config, root.path()).unwrap();
    assert!(facts.complete);
    assert!(check(&facts, &config).unwrap().is_empty());
    fs::write(root.path().join("tsconfig.json"),r#"{"compilerOptions":{"noEmit":true,"strict":true,"skipLibCheck":true},"files":["other.ts"]}"#).unwrap();
    fs::write(root.path().join("other.ts"), "export {};\n").unwrap();
    let facts = analyze(&project, &config, root.path()).unwrap();
    assert!(!facts.complete);
    assert!(!facts.file("a", "main.ts").unwrap().available);
    assert_eq!(facts.file("a", "main.ts").unwrap().properties.len(), 1);
}
#[test]
fn merged_pr_11304_reduced_before_fixed_and_partial_correction() {
    let root = tempfile::tempdir().unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/semantic-history");
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("providers/typescript7/node_modules"),
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
#[test]
fn legacy_payload_remains_strict_and_provider_version_is_pinned() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("main.ts"),
        "export const x={value:1}; x.value;",
    )
    .unwrap();
    let config = config(
        root.path(),
        json!([{"id":"a","tsconfig":"tsconfig.json","files":["*.ts"]}]),
    );
    let project = graph(root.path());
    let mut legacy = serde_json::to_value(&project).unwrap();
    assert!(legacy.get("semantic").is_none());
    legacy["semantic"] = json!({});
    assert!(serde_json::from_value::<archguard::facts::ProjectFacts>(legacy).is_err());
    let request = request(&project, &config).unwrap();
    let mut bad = clean_response(&request);
    bad.backend.version = "7.1.0".into();
    assert!(validate(&request, &bad).is_err());
}
#[test]
fn native_inherited_config_diagnostics_and_repository_plugins_are_inert() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("main.ts"),
        "declare const x: {value?:number}; x.value.toFixed();",
    )
    .unwrap();
    let config = config(
        root.path(),
        json!([{"id":"a","tsconfig":"tsconfig.json","files":["*.ts"]}]),
    );
    fs::write(
        root.path().join("base.json"),
        r#"{"compilerOptions":{"strict":true,"noEmit":true,"skipLibCheck":true}}"#,
    )
    .unwrap();
    let marker = root.path().join("executed");
    let malicious = root.path().join("plugin.cjs");
    fs::write(&malicious,format!("require('node:fs').writeFileSync({marker:?},'executed');module.exports=()=>({{create:info=>info.languageService}});")).unwrap();
    fs::write(root.path().join("tsconfig.json"),serde_json::to_string(&json!({"extends":"./base.json","compilerOptions":{"plugins":[{"name":malicious}]},"include":["main.ts"]})).unwrap()).unwrap();
    let facts = analyze(&graph(root.path()), &config, root.path()).unwrap();
    assert!(!facts.complete);
    assert!(
        facts.contexts[0]
            .diagnostics
            .iter()
            .any(|d| d.category == CompilerCategory::Error)
    );
    assert!(!marker.exists());
    fs::write(
        root.path().join("main.ts"),
        "import {missing} from './absent.ts'; missing.value;",
    )
    .unwrap();
    let facts = analyze(&graph(root.path()), &config, root.path()).unwrap();
    assert!(!facts.complete);
    assert!(facts.contexts[0].diagnostics.iter().any(|d| d.code == 2307));
    assert!(
        facts
            .file("a", "main.ts")
            .unwrap()
            .properties
            .iter()
            .any(|p| p.receiver.state == ReceiverState::Error)
    );
    fs::write(
        root.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"target":"not-a-target"},"files":["main.ts"]}"#,
    )
    .unwrap();
    let facts = analyze(&graph(root.path()), &config, root.path()).unwrap();
    assert!(!facts.complete);
    assert!(
        facts.contexts[0]
            .diagnostics
            .iter()
            .any(|d| d.phase == "config" || d.phase == "program")
    );
}
#[cfg(target_os = "linux")]
#[test]
fn deadline_kills_a_waiting_native_compiler_descendant() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("main.ts"),
        "export const x={value:1}; x.value;",
    )
    .unwrap();
    let mut config = config(
        root.path(),
        json!([{"id":"a","tsconfig":"tsconfig.json","files":["*.ts"]}]),
    );
    let executable = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("providers/typescript7/node_modules/@typescript/typescript-linux-x64/lib/tsc");
    let marker = root.path().join("native-pid");
    let script = format!(
        "const child=require('node:child_process').spawn({executable:?},['--api'],{{stdio:['pipe','ignore','ignore']}});require('node:fs').writeFileSync({marker:?},String(child.pid));setInterval(()=>{{}},1000);"
    );
    config.provider.command = vec!["node".into(), "-e".into(), script];
    config.provider.timeout_ms = 400;
    let error = analyze(&graph(root.path()), &config, root.path()).unwrap_err();
    assert!(error.to_string().contains("deadline"));
    let pid = fs::read_to_string(marker).unwrap();
    let process = Path::new("/proc").join(pid.trim()).join("stat");
    if let Ok(stat) = fs::read_to_string(process) {
        let state = stat.rsplit_once(") ").unwrap().1.chars().next().unwrap();
        assert_eq!(state, 'Z', "native compiler descendant survived deadline");
    }
}
#[test]
fn malformed_and_omitted_process_responses_are_errors() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("main.ts"),
        "export const x={value:1};x.value;",
    )
    .unwrap();
    let mut config = config(
        root.path(),
        json!([{"id":"a","tsconfig":"tsconfig.json","files":["*.ts"]}]),
    );
    let graph = graph(root.path());
    for code in [
        "process.stdout.write('{}{}')",
        "let input='';process.stdin.on('data',s=>input+=s);process.stdin.on('end',()=>{let req=JSON.parse(input);process.stdout.write(JSON.stringify({schemaVersion:1,root:req.root,backend:req.backend,complete:true,contexts:[]}));})",
        "process.stdout.write('x'.repeat(9000000))",
    ] {
        config.provider.command = vec!["node".into(), "-e".into(), code.into()];
        assert!(analyze(&graph, &config, root.path()).is_err());
    }
}
#[test]
fn semantic_incompleteness_does_not_change_legacy_plugin_graph_payload() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("main.ts"),
        "declare const untyped:any; untyped.value;",
    )
    .unwrap();
    let semantic = config(
        root.path(),
        json!([{"id":"a","tsconfig":"tsconfig.json","files":["*.ts"]}]),
    );
    let semantic_path = root.path().join("semantic.json");
    fs::write(&semantic_path, serde_json::to_vec(&semantic).unwrap()).unwrap();
    let marker = root.path().join("legacy.json");
    let script = format!(
        "let input='';process.stdin.on('data',s=>input+=s);process.stdin.on('end',()=>{{require('node:fs').writeFileSync({marker:?},input);process.stdout.write(JSON.stringify({{schemaVersion:1,diagnostics:[]}}));}});"
    );
    let graph_config = root.path().join("archguard.json");
    fs::write(&graph_config,serde_json::to_vec(&json!({"schemaVersion":1,"include":["main.ts"],"plugins":[{"name":"legacy","command":["node","-e",script]}]})).unwrap()).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_archguard"))
        .args(["check", "--root"])
        .arg(root.path())
        .arg("--config")
        .arg(&graph_config)
        .arg("--semantic-config")
        .arg(&semantic_path)
        .arg("--json")
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: archguard::facts::ProjectFacts =
        serde_json::from_slice(&fs::read(marker).unwrap()).unwrap();
    assert!(payload.problems.is_empty());
    assert_eq!(
        serde_json::to_value(&payload).unwrap()["files"],
        serde_json::to_value(graph(root.path())).unwrap()["files"]
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["complete"], false);
    assert!(!report["problems"].as_array().unwrap().is_empty());
}
