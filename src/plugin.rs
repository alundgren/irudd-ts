use crate::{config::PluginConfig, facts::*};
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::path::Path;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Response {
    schema_version: u32,
    diagnostics: Vec<Diagnostic>,
}
pub fn run(project: &ProjectFacts, plugin: &PluginConfig, cwd: &Path) -> Result<Vec<Diagnostic>> {
    crate::subprocess::run(project, plugin, cwd, |output| {
        let response: Response =
            serde_json::from_slice(output).context("expected exactly one JSON response")?;
        if response.schema_version != SCHEMA_VERSION {
            bail!(
                "unsupported response schemaVersion {}",
                response.schema_version
            );
        }
        for d in &response.diagnostics {
            let bytes = project
                .file(&d.file)
                .map(|file| file.bytes)
                .or_else(|| {
                    project
                        .packages
                        .iter()
                        .find(|p| p.path == d.file)
                        .map(|p| p.bytes)
                })
                .or_else(|| (d.file == ".").then_some(0))
                .context("diagnostic references unknown file")?;
            if d.offset > bytes || d.rule.trim().is_empty() || d.message.trim().is_empty() {
                bail!("invalid diagnostic range, rule or message");
            }
        }
        Ok(response.diagnostics)
    })
    .with_context(|| format!("plugin {}", plugin.name))
}
