use archguard::{
    config::{RuleConfig, RuleKind},
    facts::{ProjectFacts, ProjectRule},
};

fn main() -> anyhow::Result<()> {
    let project: ProjectFacts = serde_json::from_reader(std::io::stdin().lock())?;
    let rule = RuleConfig {
        id: "client-server".into(),
        kind: RuleKind::ForbiddenDependency,
        files: vec!["client/**".into()],
        targets: vec!["server/**".into()],
        specifiers: vec![],
        origins: vec![],
        names: vec![],
        transitive: true,
        include_types: true,
        exceptions: vec![],
    };
    let diagnostics = rule.check(&project)?;
    println!(
        "{}",
        serde_json::json!({"schemaVersion":1,"diagnostics":diagnostics})
    );
    Ok(())
}
