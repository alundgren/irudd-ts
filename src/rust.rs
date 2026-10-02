use crate::facts::*;
use anyhow::Result;
use syn::visit::{self, Visit};

// Rust facts support import-direction dogfooding, not type or macro expansion.
pub fn parse(path: &str, source: &str) -> Result<FileFacts> {
    let parsed = syn::parse_file(source)?;
    let mut collector = Collector {
        facts: FileFacts {
            path: path.into(),
            bytes: source.len(),
            language: "rust".into(),
            imports: vec![],
            exports: vec![],
            calls: vec![],
            services: vec![],
        },
    };
    collector.visit_file(&parsed);
    Ok(collector.facts)
}
struct Collector {
    facts: FileFacts,
}
impl Collector {
    fn imports(&mut self, tree: &syn::UseTree, prefix: &str) {
        match tree {
            syn::UseTree::Path(p) => self.imports(&p.tree, &format!("{prefix}{}::", p.ident)),
            syn::UseTree::Group(g) => {
                for item in &g.items {
                    self.imports(item, prefix);
                }
            }
            syn::UseTree::Name(n) => self.add(format!("{prefix}{}", n.ident)),
            syn::UseTree::Rename(n) => self.add(format!("{prefix}{}", n.ident)),
            syn::UseTree::Glob(_) => self.add(format!("{prefix}*")),
        }
    }
    fn add(&mut self, specifier: String) {
        self.facts.imports.push(ImportFact {
            specifier: Some(specifier),
            kind: "rustUse".into(),
            type_only: false,
            offset: 0,
            bindings: vec![],
            status: ResolutionStatus::Unresolved,
            target: None,
            detail: None,
        });
    }
}
impl<'ast> Visit<'ast> for Collector {
    fn visit_item_use(&mut self, it: &'ast syn::ItemUse) {
        self.imports(&it.tree, "");
        visit::visit_item_use(self, it);
    }
}
