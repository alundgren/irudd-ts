//! Emit an independently inventoried provider request from legacy graph facts.
use archguard::{
    facts::ProjectFacts,
    semantic::{self, SemanticConfig},
};
use std::path::Path;
fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).ok_or_else(|| {
        anyhow::anyhow!("usage: semantic_request SEMANTIC_CONFIG < project-facts.json")
    })?;
    let facts: ProjectFacts = serde_json::from_reader(std::io::stdin().lock())?;
    let (config, _) = SemanticConfig::read(Path::new(&path))?;
    serde_json::to_writer(
        std::io::stdout().lock(),
        &semantic::request(&facts, &config)?,
    )?;
    println!();
    Ok(())
}
