use archguard::{config::Config, project, rules};
use std::{fs, path::Path};

fn write(root: &Path, name: &str, source: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, source).unwrap();
}

#[test]
fn own_import_contracts_reject_policy_and_research_dependencies() {
    let mut config: Config = serde_json::from_str(include_str!("../archguard.json")).unwrap();
    // Analyze the deliberate research target so unresolved input cannot explain the finding.
    config.include.push("research/probe.ts".into());
    let root = tempfile::tempdir().unwrap();
    for name in [
        "src/facts.rs",
        "src/config.rs",
        "src/rules.rs",
        "src/roles.rs",
        "src/cache.rs",
        "src/typescript.rs",
        "src/semantic/mod.rs",
        "src/semantic/facts.rs",
        "src/semantic/inventory.rs",
        "src/semantic/validation.rs",
        "tests/roles.rs",
        "tests/cache.rs",
        "tests/semantic.rs",
    ] {
        write(root.path(), name, "");
    }
    write(root.path(), "research/probe.ts", "export const probe=1;");
    for name in ["index.ts", "semantic.ts"] {
        write(
            root.path(),
            &format!("sdk/{name}"),
            &fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("sdk").join(name))
                .unwrap(),
        );
    }
    let check = || {
        let facts = project::analyze(root.path(), &config).unwrap();
        assert!(facts.problems.is_empty(), "{:?}", facts.problems);
        rules::check(&facts, &config).unwrap()
    };
    assert!(check().is_empty());
    for (file, source, expected) in [
        ("src/facts.rs", "use crate::rules::*;", "facts-independent"),
        (
            "src/semantic/facts.rs",
            "use crate::config::Config;",
            "facts-independent",
        ),
        (
            "src/semantic/inventory.rs",
            "use crate::rules::check;",
            "semantic-inventory-no-policy",
        ),
        (
            "src/semantic/validation.rs",
            "use crate::roles::RepositoryPolicy;",
            "semantic-validation-no-policy",
        ),
        (
            "src/semantic/inventory.rs",
            "use crate::semantic::check;",
            "semantic-inventory-no-policy",
        ),
        (
            "src/semantic/validation.rs",
            "use crate::semantic::SemanticRule;",
            "semantic-validation-no-policy",
        ),
        (
            "src/typescript.rs",
            "use crate::roles::RepositoryPolicy;",
            "syntax-independent",
        ),
        (
            "examples/semantic/probe.ts",
            "import '../../research/probe.ts';",
            "product-no-research-imports",
        ),
    ] {
        write(root.path(), file, source);
        let findings = check();
        assert_eq!(findings.len(), 1, "{file}: {findings:?}");
        assert_eq!(findings[0].rule, expected);
        write(root.path(), file, "");
        assert!(check().is_empty(), "{file}");
    }
    // Shared contract imports remain allowed for syntax extraction.
    write(root.path(), "src/typescript.rs", "use crate::facts::*;");
    assert!(check().is_empty());
}
