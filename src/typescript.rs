use crate::facts::*;
use anyhow::{Result, bail};
use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_semantic::{Scoping, SemanticBuilder};
use oxc_span::SourceType;
use oxc_syntax::symbol::SymbolId;
use std::collections::HashMap;

pub fn parse(path: &str, source: &str) -> Result<FileFacts> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::from_path(path)?).parse();
    if !parsed.diagnostics.is_empty() {
        bail!(
            "{}",
            parsed
                .diagnostics
                .iter()
                .map(|d| d.message.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    let built = SemanticBuilder::new_compiler().build(&parsed.program);
    if !built.diagnostics.is_empty() {
        bail!(
            "{}",
            built
                .diagnostics
                .iter()
                .map(|d| d.message.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    let mut visitor = Collector {
        facts: FileFacts {
            path: path.into(),
            bytes: source.len(),
            language: "typescript".into(),
            imports: vec![],
            exports: vec![],
            calls: vec![],
            services: vec![],
        },
        scoping: built.semantic.scoping(),
        bindings: HashMap::new(),
    };
    // Collect imports first because an imported reference may precede its declaration.
    for statement in &parsed.program.body {
        if let Statement::ImportDeclaration(import) = statement {
            visitor.collect_import(import);
        }
        if let Statement::TSImportEqualsDeclaration(import) = statement {
            visitor.collect_import_equals(import);
        }
    }
    visitor.visit_program(&parsed.program);
    visitor.facts.imports.sort_by_key(|i| i.offset);
    visitor.facts.exports.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(visitor.facts)
}

struct Collector<'s> {
    facts: FileFacts,
    scoping: &'s Scoping,
    bindings: HashMap<SymbolId, (String, String)>,
}
impl Collector<'_> {
    fn collect_import(&mut self, it: &ImportDeclaration<'_>) {
        let mut bindings = vec![];
        let type_only = it.import_kind == ImportOrExportKind::Type;
        if let Some(specifiers) = &it.specifiers {
            for specifier in specifiers {
                let (local, imported, is_type) = match specifier {
                    ImportDeclarationSpecifier::ImportSpecifier(s) => (
                        &s.local,
                        s.imported.to_string(),
                        type_only || s.import_kind == ImportOrExportKind::Type,
                    ),
                    ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => {
                        (&s.local, "*".into(), type_only)
                    }
                    ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => {
                        (&s.local, "default".into(), type_only)
                    }
                };
                if let Some(id) = local.symbol_id.get() {
                    self.bindings
                        .insert(id, (it.source.value.to_string(), imported.clone()));
                }
                bindings.push(ImportBinding {
                    local: local.name.to_string(),
                    imported,
                    type_only: is_type,
                });
            }
        }
        let all_types = type_only || (!bindings.is_empty() && bindings.iter().all(|b| b.type_only));
        self.facts.imports.push(edge(
            Some(it.source.value.to_string()),
            "import",
            all_types,
            it.span.start,
            bindings,
        ));
    }
    fn collect_import_equals(&mut self, it: &TSImportEqualsDeclaration<'_>) {
        if let TSModuleReference::ExternalModuleReference(reference) = &it.module_reference {
            let source = reference.expression.value.to_string();
            if let Some(id) = it.id.symbol_id.get() {
                self.bindings.insert(id, (source.clone(), "*".into()));
            }
            self.facts.imports.push(edge(
                Some(source),
                "importEquals",
                it.import_kind == ImportOrExportKind::Type,
                it.span.start,
                vec![ImportBinding {
                    local: it.id.name.to_string(),
                    imported: "*".into(),
                    type_only: it.import_kind == ImportOrExportKind::Type,
                }],
            ));
        }
    }
    fn canonical(&self, expression: &Expression<'_>) -> Option<(String, Option<String>)> {
        match expression.get_inner_expression() {
            Expression::Identifier(id) => {
                let binding = id
                    .reference_id
                    .get()
                    .and_then(|r| self.scoping.get_reference(r).symbol_id())
                    .and_then(|s| self.bindings.get(&s));
                Some((
                    id.name.to_string(),
                    binding.map(|(source, imported)| format!("{source}#{imported}")),
                ))
            }
            Expression::StaticMemberExpression(member) => {
                let (base, origin) = self.canonical(&member.object)?;
                Some((
                    format!("{base}.{}", member.property.name),
                    origin.map(|s| {
                        if let Some(prefix) = s.strip_suffix("#*") {
                            format!("{prefix}#{}", member.property.name)
                        } else {
                            format!("{s}.{}", member.property.name)
                        }
                    }),
                ))
            }
            Expression::ComputedMemberExpression(member) => {
                let Expression::StringLiteral(property) = member.expression.get_inner_expression()
                else {
                    return None;
                };
                let (base, origin) = self.canonical(&member.object)?;
                Some((
                    format!("{base}.{}", property.value),
                    origin.map(|s| {
                        if let Some(prefix) = s.strip_suffix("#*") {
                            format!("{prefix}#{}", property.value)
                        } else {
                            format!("{s}.{}", property.value)
                        }
                    }),
                ))
            }

            _ => None,
        }
    }
    fn service_factory_origin(&self, expression: &Expression<'_>) -> Option<String> {
        match expression.get_inner_expression() {
            Expression::CallExpression(call) => self.service_factory_origin(&call.callee),
            other => self.canonical(other).and_then(|(_, origin)| origin),
        }
    }
    fn export(&mut self, name: String, offset: u32) {
        self.facts.exports.push(ExportFact {
            local: Some(name.clone()),
            name,
            offset: offset as usize,
        });
    }
}
fn edge(
    specifier: Option<String>,
    kind: &str,
    type_only: bool,
    offset: u32,
    bindings: Vec<ImportBinding>,
) -> ImportFact {
    ImportFact {
        specifier,
        kind: kind.into(),
        type_only,
        offset: offset as usize,
        bindings,
        status: ResolutionStatus::Unresolved,
        target: None,
        detail: None,
    }
}
fn argument_string(arg: &Argument<'_>) -> Option<String> {
    match arg {
        Argument::StringLiteral(s) => Some(s.value.to_string()),
        _ => None,
    }
}
impl<'a> Visit<'a> for Collector<'_> {
    fn visit_import_declaration(&mut self, _: &ImportDeclaration<'a>) {}
    fn visit_ts_import_equals_declaration(&mut self, _: &TSImportEqualsDeclaration<'a>) {}
    fn visit_import_expression(&mut self, it: &ImportExpression<'a>) {
        let source = match it.source.get_inner_expression() {
            Expression::StringLiteral(s) => Some(s.value.to_string()),
            _ => None,
        };
        self.facts
            .imports
            .push(edge(source, "dynamicImport", false, it.span.start, vec![]));
        walk::walk_import_expression(self, it);
    }
    fn visit_ts_import_type(&mut self, it: &TSImportType<'a>) {
        self.facts.imports.push(edge(
            Some(it.source.value.to_string()),
            "typeImport",
            true,
            it.span.start,
            vec![],
        ));
        walk::walk_ts_import_type(self, it);
    }
    fn visit_export_declaration(&mut self, it: &ExportDeclaration<'a>) {
        if let Some(id) = it.declaration.id() {
            self.export(id.name.to_string(), id.span.start);
        }
        if let Declaration::VariableDeclaration(decl) = &it.declaration {
            for variable in &decl.declarations {
                for id in variable.id.get_binding_identifiers() {
                    self.export(id.name.to_string(), id.span.start);
                }
            }
        }
        walk::walk_export_declaration(self, it);
    }
    fn visit_export_named_declaration(&mut self, it: &ExportNamedDeclaration<'a>) {
        for s in &it.specifiers {
            self.facts.exports.push(ExportFact {
                name: s.exported.to_string(),
                local: Some(s.local.to_string()),
                offset: s.span.start as usize,
            });
        }
        walk::walk_export_named_declaration(self, it);
    }
    fn visit_export_from_declaration(&mut self, it: &ExportFromDeclaration<'a>) {
        let bindings = it
            .specifiers
            .iter()
            .map(|s| ImportBinding {
                local: s.exported.to_string(),
                imported: s.local.to_string(),
                type_only: it.export_kind == ImportOrExportKind::Type
                    || s.export_kind == ImportOrExportKind::Type,
            })
            .collect::<Vec<_>>();
        let type_only = it.export_kind == ImportOrExportKind::Type
            || (!bindings.is_empty() && bindings.iter().all(|b| b.type_only));
        self.facts.imports.push(edge(
            Some(it.source.value.to_string()),
            "reExport",
            type_only,
            it.span.start,
            bindings,
        ));
        for s in &it.specifiers {
            self.export(s.exported.to_string(), s.span.start);
        }
        walk::walk_export_from_declaration(self, it);
    }
    fn visit_export_all_declaration(&mut self, it: &ExportAllDeclaration<'a>) {
        self.facts.imports.push(edge(
            Some(it.source.value.to_string()),
            if it.exported.is_some() {
                "reExportNamespace"
            } else {
                "reExportAll"
            },
            it.export_kind == ImportOrExportKind::Type,
            it.span.start,
            vec![],
        ));
        if let Some(name) = &it.exported {
            self.export(name.to_string(), it.span.start);
        }
        walk::walk_export_all_declaration(self, it);
    }
    fn visit_export_default_declaration(&mut self, it: &ExportDefaultDeclaration<'a>) {
        self.export("default".into(), it.span.start);
        walk::walk_export_default_declaration(self, it);
    }
    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if let Some((callee, origin)) = self.canonical(&it.callee) {
            if let Expression::Identifier(id) = it.callee.get_inner_expression() {
                let unbound = id
                    .reference_id
                    .get()
                    .is_some_and(|r| self.scoping.get_reference(r).symbol_id().is_none());
                if id.name == "require" && unbound {
                    self.facts.imports.push(edge(
                        it.arguments.first().and_then(argument_string),
                        "require",
                        false,
                        it.span.start,
                        vec![],
                    ));
                }
            }
            self.facts.calls.push(CallFact {
                callee,
                origin,
                offset: it.span.start as usize,
                string_arguments: it.arguments.iter().map(argument_string).collect(),
            });
        }
        walk::walk_call_expression(self, it);
    }
    fn visit_class(&mut self, it: &Class<'a>) {
        if let (Some(id), Some(heritage)) = (&it.id, &it.heritage)
            && let Some(origin) = self.service_factory_origin(&heritage.expression)
            && matches!(
                origin.as_str(),
                "effect/Context#Service"
                    | "effect/Context#Tag"
                    | "effect#Context.Service"
                    | "effect#Context.Tag"
                    | "effect/ServiceMap#Service"
            )
        {
            let identifier = match heritage.expression.get_inner_expression() {
                Expression::CallExpression(call) => {
                    call.arguments.first().and_then(argument_string)
                }
                _ => None,
            };
            self.facts.services.push(ServiceFact {
                name: id.name.to_string(),
                identifier,
                offset: it.span.start as usize,
            });
        }
        walk::walk_class(self, it);
    }
}
