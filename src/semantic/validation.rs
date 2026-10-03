use crate::semantic::{
    API_ID, BACKEND_VERSION, CompilerCategory, MemberStatus, ReceiverState, SCHEMA_VERSION,
    SemanticFacts, SemanticRequest, hash,
};
use anyhow::{Context, Result, bail};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub fn validate(request: &SemanticRequest, facts: &SemanticFacts) -> Result<()> {
    if request.schema_version != SCHEMA_VERSION
        || request.backend.name != "typescript"
        || request.backend.version != BACKEND_VERSION
        || request.backend.api != API_ID
        || facts.schema_version != SCHEMA_VERSION
        || facts.root != request.root
        || facts.backend != request.backend
    {
        bail!("semantic protocol/backend/root mismatch");
    }
    let expected: BTreeMap<_, _> = request
        .contexts
        .iter()
        .map(|c| (c.id.as_str(), c))
        .collect();
    if expected.is_empty() || expected.len() != request.contexts.len() {
        bail!("invalid requested compiler contexts");
    }
    let mut seen = BTreeSet::new();
    let mut all_complete = true;
    for context in &facts.contexts {
        let wanted = expected
            .get(context.id.as_str())
            .context("unknown semantic context")?;
        if !seen.insert(&context.id)
            || context.tsconfig != wanted.tsconfig
            || context.config_sha256 != wanted.config_sha256
        {
            bail!("duplicate or mismatched semantic context");
        }
        if hash(&std::fs::read(&context.tsconfig)?) != wanted.config_sha256 {
            bail!("compiler configuration changed during analysis");
        }
        let files: BTreeMap<_, _> = wanted.files.iter().map(|f| (f.path.as_str(), f)).collect();
        let mut seen_files = BTreeSet::new();
        let mut complete = context.problems.is_empty()
            && !context
                .diagnostics
                .iter()
                .any(|d| d.category == CompilerCategory::Error);
        for diagnostic in &context.diagnostics {
            if !["config", "program", "global", "syntax", "bind", "semantic"]
                .contains(&diagnostic.phase.as_str())
                || diagnostic.code == 0
                || diagnostic.message.trim().is_empty()
                || diagnostic.end < diagnostic.offset
                || diagnostic
                    .file
                    .as_ref()
                    .is_some_and(|f| f.trim().is_empty())
            {
                bail!("invalid compiler diagnostic");
            }
            if let Some(file) = &diagnostic.file {
                validate_location(&request.root, file, diagnostic.offset, diagnostic.end)?;
            }
        }
        for file in &context.files {
            let wanted = files
                .get(file.path.as_str())
                .context("extra semantic file")?;
            if !seen_files.insert(&file.path)
                || file.bytes != wanted.bytes
                || file.sha256 != wanted.sha256
            {
                bail!("duplicate or mismatched semantic file");
            }
            let source = std::fs::read(Path::new(&request.root).join(&file.path))?;
            if hash(&source) != wanted.sha256 {
                bail!("source changed during semantic analysis");
            }
            let sites: BTreeMap<_, _> = wanted
                .sites
                .iter()
                .map(|s| (s.offset, s.member.as_str()))
                .collect();
            let mut seen_sites = BTreeSet::new();
            complete &= file.available;
            for property in &file.properties {
                if sites.get(&property.offset) != Some(&property.member.as_str())
                    || !seen_sites.insert(property.offset)
                {
                    bail!("extra, duplicate or mismatched semantic property site");
                }
                let known = property.receiver.state == ReceiverState::Known;
                let usable = property.status != MemberStatus::Unavailable;
                if known != usable
                    || known
                        && property
                            .receiver
                            .display
                            .as_ref()
                            .is_none_or(|s| s.trim().is_empty())
                    || !file.available && usable
                    || property.status == MemberStatus::Present
                        && property
                            .symbol
                            .as_ref()
                            .is_none_or(|s| s.name != property.member)
                    || property.status != MemberStatus::Present && property.symbol.is_some()
                    || !usable && property.detail.as_ref().is_none_or(|s| s.trim().is_empty())
                {
                    bail!("contradictory semantic property fact");
                }
                if let Some(symbol) = &property.symbol {
                    for declaration in &symbol.declarations {
                        if declaration.file.trim().is_empty() {
                            bail!("invalid semantic declaration location");
                        }
                        validate_location(
                            &request.root,
                            &declaration.file,
                            declaration.offset,
                            declaration.offset,
                        )?;
                    }
                }
                complete &= usable;
            }
            if seen_sites.len() != sites.len() {
                bail!("provider omitted semantic property sites");
            }
        }
        if seen_files.len() != files.len() {
            bail!("provider omitted semantic files");
        }
        if context.complete != complete {
            bail!("forged semantic context completion");
        }
        all_complete &= complete;
    }
    if seen.len() != expected.len() {
        bail!("provider omitted compiler contexts");
    }
    if facts.complete != all_complete {
        bail!("forged semantic completion");
    }
    Ok(())
}
fn validate_location(root: &str, file: &str, offset: usize, end: usize) -> Result<()> {
    let source = std::fs::read_to_string(Path::new(root).join(file))
        .with_context(|| format!("semantic location source {file}"))?;
    if end < offset
        || end > source.len()
        || !source.is_char_boundary(offset)
        || !source.is_char_boundary(end)
    {
        bail!("invalid semantic UTF-8 location in {file}");
    }
    Ok(())
}
