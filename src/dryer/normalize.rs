use crate::{
    dryer::{
        facts::{NodeKey, NormalizedFunction},
        *,
    },
    quality::{
        functions::candidates,
        load::{issue, push_evidence},
        syntax::parse,
        *,
    },
};
use anyhow::Result;
use oxc_ast::AstKind;
use oxc_semantic::Semantic;
use oxc_span::GetSpan;
use oxc_syntax::{node::NodeId, symbol::SymbolId};
use std::collections::HashMap;

struct Context<'s, 'a> {
    semantic: &'s Semantic<'a>,
    source: &'s str,
    options: &'s NormalizationOptions,
    locals: HashMap<SymbolId, usize>,
    captures: HashMap<SymbolId, usize>,
}
impl Context<'_, '_> {
    fn identifier(&mut self, name: &str, symbol: Option<SymbolId>, called: bool) -> String {
        if called {
            return format!("called:{name}");
        }
        if self.options.local_identifiers == LocalIdentifiers::Erase {
            return "identifier".into();
        }
        if let Some(symbol) = symbol {
            if let Some(ordinal) = self.locals.get(&symbol) {
                return format!("local:{ordinal}");
            }
            let next = self.captures.len();
            return format!("capture:{}", self.captures.entry(symbol).or_insert(next));
        }
        format!("free:{name}")
    }
    fn called(&self, id: NodeId) -> bool {
        let span = self.semantic.nodes().kind(id).span();
        self.semantic.nodes().ancestor_kinds(id).any(|kind| {
            let target = match kind {
                AstKind::CallExpression(call) => Some(call.callee.span()),
                AstKind::NewExpression(call) => Some(call.callee.span()),
                AstKind::TaggedTemplateExpression(tag) => Some(tag.tag.span()),
                _ => None,
            };
            target.is_some_and(|target| target.start <= span.start && target.end >= span.end)
        })
    }
    fn called_identifier(&self, id: NodeId) -> bool {
        let nodes = self.semantic.nodes();
        let mut current = id;
        for parent in nodes.ancestor_ids(id) {
            let target = match nodes.kind(parent) {
                AstKind::CallExpression(call) => Some(call.callee.span()),
                AstKind::NewExpression(call) => Some(call.callee.span()),
                AstKind::TaggedTemplateExpression(tag) => Some(tag.tag.span()),
                AstKind::ParenthesizedExpression(_)
                | AstKind::TSNonNullExpression(_)
                | AstKind::TSInstantiationExpression(_) => {
                    current = parent;
                    continue;
                }
                _ => return false,
            };
            return target.is_some_and(|span| span == nodes.kind(current).span());
        }
        false
    }
    fn property_key(&self, id: NodeId) -> bool {
        let span = self.semantic.nodes().kind(id).span();
        let key = match self.semantic.nodes().parent_kind(id) {
            AstKind::ObjectProperty(property) => Some(property.key.span()),
            AstKind::BindingProperty(property) => Some(property.key.span()),
            AstKind::ComputedMemberExpression(member) => Some(member.expression.span()),
            _ => None,
        };
        key.is_some_and(|key| key.start <= span.start && key.end >= span.end)
    }
    fn scalar(&mut self, id: NodeId) -> Option<String> {
        let kind = self.semantic.nodes().kind(id);
        let scalar = match kind {
            AstKind::IdentifierReference(identifier) => {
                let symbol = identifier.reference_id.get().and_then(|reference| {
                    self.semantic.scoping().get_reference(reference).symbol_id()
                });
                let direct_call = self.called_identifier(id);
                self.identifier(identifier.name.as_str(), symbol, direct_call)
            }
            AstKind::BindingIdentifier(identifier) => {
                self.identifier(identifier.name.as_str(), identifier.symbol_id.get(), false)
            }
            AstKind::IdentifierName(identifier) => {
                if self.options.properties == PropertyNames::Erase && !self.called(id) {
                    "property".into()
                } else {
                    identifier.name.to_string()
                }
            }
            AstKind::PrivateIdentifier(identifier) => {
                if self.options.properties == PropertyNames::Erase && !self.called(id) {
                    "private".into()
                } else {
                    identifier.name.to_string()
                }
            }
            AstKind::NumericLiteral(_)
            | AstKind::StringLiteral(_)
            | AstKind::BooleanLiteral(_)
            | AstKind::BigIntLiteral(_) => {
                if self.options.literals == LiteralValues::Value
                    || (self.called(id) && self.property_key(id))
                    || (self.property_key(id) && self.options.properties == PropertyNames::Preserve)
                {
                    let span = kind.span();
                    self.source[span.start as usize..span.end as usize].into()
                } else {
                    String::new()
                }
            }
            AstKind::BinaryExpression(expression) => expression.operator.as_str().into(),
            AstKind::LogicalExpression(expression) => expression.operator.as_str().into(),
            AstKind::AssignmentExpression(expression) => expression.operator.as_str().into(),
            AstKind::UnaryExpression(expression) => expression.operator.as_str().into(),
            AstKind::UpdateExpression(expression) => {
                serde_json::to_string(&(expression.operator.as_str(), expression.prefix)).unwrap()
            }
            AstKind::CallExpression(expression) => {
                serde_json::to_string(&(expression.optional, expression.type_arguments.is_some()))
                    .unwrap()
            }
            AstKind::NewExpression(expression) => {
                serde_json::to_string(&expression.type_arguments.is_some()).unwrap()
            }
            AstKind::StaticMemberExpression(expression) => expression.optional.to_string(),
            AstKind::ComputedMemberExpression(expression) => expression.optional.to_string(),
            AstKind::PrivateFieldExpression(expression) => expression.optional.to_string(),
            AstKind::ObjectProperty(property) => serde_json::to_string(&(
                format!("{:?}", property.kind),
                property.method,
                property.shorthand,
                property.computed,
            ))
            .unwrap(),
            AstKind::BindingProperty(property) => {
                serde_json::to_string(&(property.shorthand, property.computed)).unwrap()
            }
            AstKind::VariableDeclaration(variable) => {
                serde_json::to_string(&(format!("{:?}", variable.kind), variable.declare)).unwrap()
            }
            AstKind::VariableDeclarator(variable) => serde_json::to_string(&(
                variable.init.is_some(),
                variable.type_annotation.is_some(),
                variable.definite,
            ))
            .unwrap(),
            AstKind::Function(function) => serde_json::to_string(&(
                format!("{:?}", function.r#type),
                function.id.is_some(),
                function.r#async,
                function.generator,
                function.declare,
                function.type_parameters.is_some(),
                function.this_param.is_some(),
                function.return_type.is_some(),
                function.body.is_some(),
            ))
            .unwrap(),
            AstKind::ArrowFunctionExpression(function) => serde_json::to_string(&(
                function.r#async,
                function.body.is_function_body(),
                function.type_parameters.is_some(),
                function.return_type.is_some(),
            ))
            .unwrap(),
            AstKind::FormalParameters(parameters) => serde_json::to_string(&(
                format!("{:?}", parameters.kind),
                parameters.rest.is_some(),
            ))
            .unwrap(),
            AstKind::FormalParameter(parameter) if parameter.decorators.is_empty() => {
                serde_json::to_string(&(
                    parameter.type_annotation.is_some(),
                    parameter.initializer.is_some(),
                    parameter.optional,
                    format!("{:?}", parameter.accessibility),
                    parameter.readonly,
                    parameter.r#override,
                ))
                .unwrap()
            }
            AstKind::FormalParameterRest(parameter) if parameter.decorators.is_empty() => {
                parameter.type_annotation.is_some().to_string()
            }
            AstKind::ForStatement(statement) => serde_json::to_string(&(
                statement.init.is_some(),
                statement.test.is_some(),
                statement.update.is_some(),
            ))
            .unwrap(),
            AstKind::ForOfStatement(statement) => statement.r#await.to_string(),
            AstKind::IfStatement(statement) => statement.alternate.is_some().to_string(),
            AstKind::ReturnStatement(statement) => statement.argument.is_some().to_string(),
            AstKind::YieldExpression(expression) => {
                serde_json::to_string(&(expression.delegate, expression.argument.is_some()))
                    .unwrap()
            }
            AstKind::TryStatement(statement) => {
                serde_json::to_string(&(statement.handler.is_some(), statement.finalizer.is_some()))
                    .unwrap()
            }
            AstKind::CatchClause(statement) => statement.param.is_some().to_string(),
            AstKind::CatchParameter(parameter) => parameter.type_annotation.is_some().to_string(),
            AstKind::ArrayPattern(pattern) => serde_json::to_string(&(
                pattern
                    .elements
                    .iter()
                    .map(Option::is_some)
                    .collect::<Vec<_>>(),
                pattern.rest.is_some(),
            ))
            .unwrap(),
            AstKind::ObjectPattern(pattern) => pattern.rest.is_some().to_string(),
            AstKind::SwitchCase(statement) => statement.test.is_some().to_string(),
            AstKind::BreakStatement(statement) => statement.label.is_some().to_string(),
            AstKind::ContinueStatement(statement) => statement.label.is_some().to_string(),
            AstKind::ThisExpression(_)
            | AstKind::NullLiteral(_)
            | AstKind::Elision(_)
            | AstKind::Super(_)
            | AstKind::ArrayExpression(_)
            | AstKind::ObjectExpression(_)
            | AstKind::SpreadElement(_)
            | AstKind::ConditionalExpression(_)
            | AstKind::SequenceExpression(_)
            | AstKind::AwaitExpression(_)
            | AstKind::ChainExpression(_)
            | AstKind::ParenthesizedExpression(_)
            | AstKind::BlockStatement(_)
            | AstKind::FunctionBody(_)
            | AstKind::ExpressionStatement(_)
            | AstKind::EmptyStatement(_)
            | AstKind::DoWhileStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::ForInStatement(_)
            | AstKind::ThrowStatement(_)
            | AstKind::SwitchStatement(_)
            | AstKind::DebuggerStatement(_)
            | AstKind::AssignmentPattern(_)
            | AstKind::BindingRestElement(_) => String::new(),
            _ => return None,
        };
        Some(scalar)
    }
}

pub fn extract(
    path: &str,
    source: &str,
    options: &NormalizationOptions,
    limits: &AnalysisLimits,
) -> Result<FunctionInventory> {
    options.validate()?;
    limits.validate()?;
    let options_owned = options.clone();
    let limits_owned = limits.clone();
    let path_owned = path.to_owned();
    let parsed = parse(path, source, limits, move |semantic, source| {
        let nodes = semantic.nodes();
        let mut children = vec![vec![]; nodes.len()];
        for node in nodes.iter().skip(1) {
            children[nodes.parent_id(node.id()).index()].push(node.id());
        }
        let mut result = FunctionInventory {
            normalization: options_owned.clone(),
            functions: vec![],
            excluded: vec![],
            complete: true,
            problems: vec![],
            omitted_evidence: OmittedEvidence::default(),
            normalized: vec![],
            nodes: nodes.len(),
        };
        let mut evidence_used = 0;
        for (root, function) in candidates(semantic, &path_owned, source, &limits_owned) {
            if result.normalized.len() >= limits_owned.max_candidates {
                result.complete = false;
                result.omitted_evidence.functions += 1;
                result.problems.push(issue(
                    ProblemKind::AnalysisLimit,
                    &path_owned,
                    function.location.start,
                    "function candidate limit reached",
                    Some(AnalysisLimitKind::Candidates),
                    &limits_owned,
                ));
                break;
            }
            let root_span = nodes.kind(root).span();
            let mut symbols: Vec<_> = semantic
                .scoping()
                .symbol_ids()
                .filter(|id| {
                    let span = semantic.scoping().symbol_span(*id);
                    span.start >= root_span.start && span.end <= root_span.end
                })
                .collect();
            symbols.sort_by_key(|id| (semantic.scoping().symbol_span(*id).start, id.index()));
            let mut context = Context {
                semantic,
                source,
                options: &options_owned,
                locals: symbols
                    .into_iter()
                    .enumerate()
                    .map(|(ordinal, id)| (id, ordinal))
                    .collect(),
                captures: HashMap::new(),
            };
            let mut pending = vec![root];
            let mut visited = vec![];
            let mut opaque_nodes = 0;
            while let Some(id) = pending.pop() {
                let kind = nodes.kind(id);
                let (scalar, opaque) = match context.scalar(id) {
                    Some(scalar) => (scalar, false),
                    None => {
                        let span = kind.span();
                        opaque_nodes += 1;
                        (source[span.start as usize..span.end as usize].into(), true)
                    }
                };
                visited.push((id, scalar, opaque));
                if !opaque {
                    pending.extend(children[id.index()].iter().rev().copied());
                }
            }
            let mut tree = vec![];
            let mut mapped = HashMap::new();
            for (id, scalar, opaque) in visited.into_iter().rev() {
                let child_ids = if opaque {
                    vec![]
                } else {
                    children[id.index()].iter().map(|id| mapped[id]).collect()
                };
                let index = tree.len();
                tree.push(NodeKey {
                    kind: format!(
                        "{}{:?}",
                        if opaque { "Opaque:" } else { "" },
                        nodes.kind(id).ty()
                    ),
                    scalar,
                    children: child_ids,
                });
                mapped.insert(id, index);
            }
            // Getter/setter/static method distinctions belong to the selected definition.
            let metadata = match nodes.parent_kind(root) {
                AstKind::MethodDefinition(method) => Some(
                    serde_json::to_string(&(
                        format!("{:?}", method.kind),
                        method.computed,
                        method.r#static,
                        method.optional,
                        method.r#override,
                        format!("{:?}", method.accessibility),
                    ))
                    .unwrap(),
                ),
                AstKind::ObjectProperty(property) => Some(
                    serde_json::to_string(&(
                        format!("{:?}", property.kind),
                        property.computed,
                        property.method,
                    ))
                    .unwrap(),
                ),
                AstKind::PropertyDefinition(property) => Some(
                    serde_json::to_string(&(
                        property.computed,
                        property.r#static,
                        property.optional,
                        property.readonly,
                        property.declare,
                        property.definite,
                        property.r#override,
                        format!("{:?}", property.accessibility),
                    ))
                    .unwrap(),
                ),
                _ => None,
            };
            if let Some(metadata) = metadata {
                let root = tree.last_mut().expect("function has root");
                root.scalar = serde_json::to_string(&(&root.scalar, metadata)).unwrap();
            }
            let facts = FunctionFacts {
                function,
                nodes: tree.len(),
                opaque_nodes,
            };
            if !push_evidence(
                &mut result.functions,
                facts.clone(),
                &mut evidence_used,
                &limits_owned,
            ) {
                result.complete = false;
                result.omitted_evidence.functions += 1;
                result.problems.push(issue(
                    ProblemKind::ReportLimit,
                    &path_owned,
                    facts.function.location.start,
                    "function evidence byte budget reached",
                    Some(AnalysisLimitKind::ReportBytes),
                    &limits_owned,
                ));
                break;
            }
            result.normalized.push(NormalizedFunction { facts, tree });
        }
        result
    })?;
    let mut result = parsed.value.unwrap_or(FunctionInventory {
        normalization: options.clone(),
        functions: vec![],
        excluded: vec![],
        complete: false,
        problems: vec![],
        omitted_evidence: OmittedEvidence::default(),
        normalized: vec![],
        nodes: parsed.nodes,
    });
    result.problems.extend(parsed.problems);
    result.complete &= result.problems.is_empty();
    Ok(result)
}
