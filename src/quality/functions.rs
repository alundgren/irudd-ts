use crate::quality::{
    load::{location, truncate},
    *,
};
use oxc_ast::{AstKind, ast::BindingPattern};
use oxc_semantic::Semantic;
use oxc_span::GetSpan;
use oxc_syntax::node::NodeId;

pub(crate) fn candidates(
    semantic: &Semantic<'_>,
    path: &str,
    source: &str,
    limits: &AnalysisLimits,
) -> Vec<(NodeId, FunctionLocation)> {
    let nodes = semantic.nodes();
    let mut result = vec![];
    for node in nodes.iter() {
        let arrow = match node.kind() {
            AstKind::Function(function) if function.body.is_some() && !function.declare => false,
            AstKind::ArrowFunctionExpression(_) => true,
            _ => continue,
        };
        if ambient(semantic, node.id()) {
            continue;
        }
        if nodes.ancestor_kinds(node.id()).any(|kind| {
            matches!(
                kind,
                AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
            )
        }) {
            continue;
        }
        let parent = nodes.parent_kind(node.id());
        let selected = match (node.kind(), parent) {
            (AstKind::Function(function), _) if function.is_function_declaration() => Some((
                function
                    .id
                    .as_ref()
                    .map_or("default", |id| id.name.as_str())
                    .to_owned(),
                FunctionKind::FunctionDeclaration,
                function.span,
            )),
            (_, AstKind::VariableDeclarator(variable)) => match &variable.id {
                BindingPattern::BindingIdentifier(id) => Some((
                    id.name.to_string(),
                    if arrow {
                        FunctionKind::VariableArrow
                    } else {
                        FunctionKind::VariableFunction
                    },
                    variable.span,
                )),
                _ => None,
            },
            (_, AstKind::MethodDefinition(method)) => Some((
                method
                    .key
                    .static_name()
                    .map_or_else(|| "[computed]".into(), |name| name.into_owned()),
                FunctionKind::Method,
                method.span,
            )),
            (_, AstKind::ObjectProperty(property))
                if property.method
                    || !matches!(property.kind, oxc_ast::ast::PropertyKind::Init) =>
            {
                Some((
                    property
                        .key
                        .static_name()
                        .map_or_else(|| "[computed]".into(), |name| name.into_owned()),
                    FunctionKind::Method,
                    property.span,
                ))
            }
            (_, AstKind::PropertyDefinition(property)) => Some((
                property
                    .key
                    .static_name()
                    .map_or_else(|| "[computed]".into(), |name| name.into_owned()),
                if arrow {
                    FunctionKind::FieldArrow
                } else {
                    FunctionKind::FieldFunction
                },
                property.span,
            )),
            _ => None,
        };
        if let Some((name, kind, span)) = selected {
            let (name, name_truncated) = truncate(&name, limits.max_name_bytes);
            if result.len() > limits.max_candidates {
                break;
            }
            result.push((
                node.id(),
                FunctionLocation {
                    name,
                    kind,
                    location: location(path, source, span.start as usize, span.end as usize),
                    name_truncated,
                },
            ));
        }
    }
    result
}

fn ambient(semantic: &Semantic<'_>, id: NodeId) -> bool {
    semantic.nodes().ancestor_kinds(id).any(|kind| {
        matches!(kind, AstKind::VariableDeclaration(variable) if variable.declare)
            || matches!(kind, AstKind::Class(class) if class.declare)
            || matches!(kind, AstKind::TSEnumDeclaration(enumeration) if enumeration.declare)
            || matches!(kind, AstKind::TSNamespaceDeclaration(namespace) if namespace.declare)
            || matches!(
                kind,
                AstKind::TSExternalModuleDeclaration(_) | AstKind::TSGlobalDeclaration(_)
            )
    })
}

pub(crate) fn runtime_node(semantic: &Semantic<'_>, id: NodeId) -> bool {
    if ambient(semantic, id) {
        return false;
    }
    let nodes = semantic.nodes();
    let span = nodes.kind(id).span();
    for kind in nodes.ancestor_kinds(id) {
        let type_name = format!("{:?}", kind.ty());
        if type_name.starts_with("TS")
            && !matches!(
                kind,
                AstKind::TSAsExpression(_)
                    | AstKind::TSSatisfiesExpression(_)
                    | AstKind::TSNonNullExpression(_)
                    | AstKind::TSInstantiationExpression(_)
                    | AstKind::TSExportAssignment(_)
                    | AstKind::TSEnumDeclaration(_)
                    | AstKind::TSEnumBody(_)
                    | AstKind::TSEnumMember(_)
                    | AstKind::TSNamespaceDeclaration(_)
                    | AstKind::TSModuleBlock(_)
            )
        {
            return false;
        }
        let key = match kind {
            AstKind::ObjectProperty(property) if !property.computed => Some(property.key.span()),
            AstKind::BindingProperty(property) if !property.computed => Some(property.key.span()),
            AstKind::MethodDefinition(method) if !method.computed => Some(method.key.span()),
            AstKind::PropertyDefinition(property) if !property.computed => {
                Some(property.key.span())
            }
            AstKind::TSEnumMember(member) => Some(member.id.span()),
            _ => None,
        };
        if key.is_some_and(|key| key.start <= span.start && key.end >= span.end) {
            return false;
        }
    }
    true
}
