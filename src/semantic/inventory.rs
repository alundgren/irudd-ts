use crate::{
    config::Matcher,
    facts::ProjectFacts,
    semantic::{
        API_ID, BACKEND_VERSION, Backend, PropertySite, RequestedContext, RequestedFile,
        SCHEMA_VERSION, SemanticConfig, SemanticRequest,
    },
};
use anyhow::{Context, Result, bail};
use oxc_allocator::Allocator;
use oxc_ast::ast::{JSXMemberExpression, StaticMemberExpression};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_span::SourceType;
use sha2::{Digest, Sha256};
use std::path::Path;

pub fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
pub fn inventory(path: &str, source: &str) -> Result<Vec<PropertySite>> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::from_path(path)?).parse();
    if !parsed.diagnostics.is_empty() {
        bail!(
            "semantic site inventory parse errors: {}",
            parsed
                .diagnostics
                .iter()
                .map(|d| d.message.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    struct Sites(Vec<PropertySite>);
    impl<'a> Visit<'a> for Sites {
        fn visit_static_member_expression(&mut self, it: &StaticMemberExpression<'a>) {
            self.0.push(PropertySite {
                offset: it.property.span.start as usize,
                member: it.property.name.to_string(),
            });
            walk::walk_static_member_expression(self, it);
        }
        fn visit_jsx_member_expression(&mut self, it: &JSXMemberExpression<'a>) {
            self.0.push(PropertySite {
                offset: it.property.span.start as usize,
                member: it.property.name.to_string(),
            });
            walk::walk_jsx_member_expression(self, it);
        }
    }
    let mut sites = Sites(vec![]);
    sites.visit_program(&parsed.program);
    sites.0.sort_by_key(|s| s.offset);
    Ok(sites.0)
}
pub fn request(project: &ProjectFacts, config: &SemanticConfig) -> Result<SemanticRequest> {
    config.validate()?;
    if project.schema_version != crate::facts::SCHEMA_VERSION {
        bail!("unsupported source graph facts version");
    }
    let root = Path::new(&project.root).canonicalize()?;
    let mut contexts = vec![];
    for context in &config.contexts {
        let matcher = Matcher::new(&context.files)?;
        let tsconfig = root
            .join(&context.tsconfig)
            .canonicalize()
            .with_context(|| format!("compiler context {} tsconfig", context.id))?;
        let config_sha256 = hash(&std::fs::read(&tsconfig)?);
        let mut files = vec![];
        for file in &project.files {
            if !matcher.matches(&file.path) {
                continue;
            }
            if file.language != "typescript" {
                bail!(
                    "semantic context {} selected unsupported file {}",
                    context.id,
                    file.path
                );
            }
            let source = std::fs::read_to_string(root.join(&file.path))?;
            if source.len() != file.bytes {
                bail!("source changed since graph analysis: {}", file.path);
            }
            files.push(RequestedFile {
                path: file.path.clone(),
                bytes: source.len(),
                sha256: hash(source.as_bytes()),
                sites: inventory(&file.path, &source)?,
            });
        }
        if files.is_empty() {
            bail!(
                "compiler context {} selects no analyzed source files",
                context.id
            );
        }
        contexts.push(RequestedContext {
            id: context.id.clone(),
            tsconfig: tsconfig.to_string_lossy().into(),
            config_sha256,
            files,
        });
    }
    Ok(SemanticRequest {
        schema_version: SCHEMA_VERSION,
        root: root.to_string_lossy().into(),
        backend: Backend {
            name: "typescript".into(),
            version: BACKEND_VERSION.into(),
            api: API_ID.into(),
        },
        contexts,
    })
}
