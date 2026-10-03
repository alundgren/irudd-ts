use archguard::semantic::*;
use serde_json::json;
use std::fs;
#[path = "support/semantic.rs"]
mod support;
use support::{config, graph};

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
