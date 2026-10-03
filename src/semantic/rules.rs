use crate::{
    config::Matcher,
    facts::Diagnostic,
    semantic::{MemberRule, MemberStatus, ReceiverState, SemanticConfig, SemanticFacts},
};
use anyhow::Result;

pub trait SemanticRule {
    fn check(&self, facts: &SemanticFacts) -> Result<Vec<Diagnostic>>;
}
impl SemanticRule for MemberRule {
    fn check(&self, facts: &SemanticFacts) -> Result<Vec<Diagnostic>> {
        let matcher = Matcher::new(&self.files)?;
        let mut findings = vec![];
        for context in &facts.contexts {
            for file in &context.files {
                if !matcher.matches(&file.path) {
                    continue;
                }
                for fact in &file.properties {
                    if fact.receiver.state == ReceiverState::Known
                        && fact.status == MemberStatus::Missing
                    {
                        let mut d = Diagnostic::new(
                            &self.id,
                            &file.path,
                            fact.offset,
                            format!(
                                "member {} is absent from the inferred receiver type",
                                fact.member
                            ),
                        );
                        d.evidence = vec![
                            format!("compiler context {}", context.id),
                            fact.receiver.display.clone().unwrap_or_default(),
                        ];
                        findings.push(d);
                    }
                }
            }
        }
        findings.sort();
        findings.dedup();
        Ok(findings)
    }
}
pub fn check(facts: &SemanticFacts, config: &SemanticConfig) -> Result<Vec<Diagnostic>> {
    let mut result = vec![];
    for rule in &config.rules {
        result.extend(rule.check(facts)?);
    }
    result.sort();
    result.dedup();
    Ok(result)
}
