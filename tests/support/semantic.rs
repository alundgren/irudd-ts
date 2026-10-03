use archguard::{config::Config, project, semantic::SemanticConfig};
use serde_json::json;
use std::{fs, path::Path};

pub fn config(root: &Path, contexts: serde_json::Value) -> SemanticConfig {
    let provider = Path::new(env!("CARGO_MANIFEST_DIR")).join("providers/typescript7/provider.mjs");
    fs::write(root.join("tsconfig.json"),r#"{"compilerOptions":{"strict":true,"noEmit":true,"skipLibCheck":true,"target":"ESNext","module":"NodeNext","moduleResolution":"NodeNext"},"include":["*.ts"]}"#).unwrap();
    serde_json::from_value(json!({"schemaVersion":1,"provider":{"name":"typescript7","command":["node",provider],"timeoutMs":30000},"contexts":contexts,"rules":[{"id":"missing-member","kind":"missingMember","files":["**"]}]})).unwrap()
}
pub fn graph(root: &Path) -> archguard::facts::ProjectFacts {
    project::analyze(
        root,
        &serde_json::from_value::<Config>(json!({"schemaVersion":1,"include":["*.ts"]})).unwrap(),
    )
    .unwrap()
}
