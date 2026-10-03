use archguard::{config, facts::ProjectFacts};

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("provide a configuration path"))?;
    let (config, _) = config::read(std::path::Path::new(&path))?;
    let policy = config
        .repository
        .ok_or_else(|| anyhow::anyhow!("configuration has no repository roles"))?;
    let facts: ProjectFacts = serde_json::from_reader(std::io::stdin().lock())?;
    anyhow::ensure!(
        facts.schema_version == archguard::facts::SCHEMA_VERSION,
        "unsupported facts protocol"
    );
    serde_json::to_writer_pretty(std::io::stdout().lock(), &policy.assignments(&facts)?)?;
    Ok(())
}
