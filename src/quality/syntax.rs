use crate::quality::{load::issue, *};
use anyhow::Result;
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_semantic::{Semantic, SemanticBuilder};
use oxc_span::SourceType;
use std::sync::{Condvar, Mutex};

pub const PARSER_VERSION: &str = "oxc-0.152.0";
const PARSER_STACK_BYTES: usize = 128 * 1024 * 1024;
static PARSERS: (Mutex<usize>, Condvar) = (Mutex::new(0), Condvar::new());
struct ParserPermit;
impl ParserPermit {
    fn acquire() -> Self {
        let mut count = PARSERS.0.lock().unwrap_or_else(|error| error.into_inner());
        while *count >= AnalysisLimits::hard_maximum().workers {
            count = PARSERS
                .1
                .wait(count)
                .unwrap_or_else(|error| error.into_inner());
        }
        *count += 1;
        Self
    }
}
impl Drop for ParserPermit {
    fn drop(&mut self) {
        let mut count = PARSERS.0.lock().unwrap_or_else(|error| error.into_inner());
        *count -= 1;
        PARSERS.1.notify_one();
    }
}

pub(crate) fn preflight(
    path: &str,
    source: &str,
    limits: &AnalysisLimits,
) -> Option<AnalysisProblem> {
    if source.len() > limits.max_file_bytes {
        return Some(issue(
            ProblemKind::AnalysisLimit,
            path,
            0,
            "source exceeds file byte limit",
            Some(AnalysisLimitKind::FileBytes),
            limits,
        ));
    }
    let mut units = 0usize;
    let mut depth = 0usize;
    let mut run = false;
    let mut recursive_operators = 0usize;
    let mut identifier_start = 0usize;
    let mut previous = 0u8;
    for (offset, byte) in source.bytes().enumerate() {
        let identifier = byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$';
        if identifier {
            if !run {
                identifier_start = offset;
                units += 1;
            }
        } else if !byte.is_ascii_whitespace() {
            units += 1;
        }
        if !identifier
            && run
            && matches!(
                &source[identifier_start..offset],
                "await"
                    | "yield"
                    | "typeof"
                    | "void"
                    | "delete"
                    | "new"
                    | "in"
                    | "instanceof"
                    | "as"
                    | "satisfies"
            )
        {
            recursive_operators += 1;
        }
        run = identifier;
        if units > limits.max_raw_units {
            return Some(issue(
                ProblemKind::SourceComplexityLimit,
                path,
                offset,
                "conservative raw source unit limit reached",
                Some(AnalysisLimitKind::RawUnits),
                limits,
            ));
        }
        match byte {
            b';' | b'{' | b'}' | b',' => recursive_operators = 0,
            b'!' | b'~' | b'=' | b'?' | b'+' | b'-' | b'*' | b'/' | b'%' | b'&' | b'|' | b'^'
            | b'<' | b'>' | b':' | b'.' => recursive_operators += 1,
            b'(' if previous.is_ascii_alphanumeric()
                || matches!(previous, b')' | b']' | b'_' | b'$') =>
            {
                recursive_operators += 1
            }
            _ => {}
        }
        if recursive_operators > limits.max_delimiter_depth {
            return Some(issue(
                ProblemKind::SourceComplexityLimit,
                path,
                offset,
                &format!(
                    "conservative recursive operator depth budget {} reached",
                    limits.max_delimiter_depth
                ),
                Some(AnalysisLimitKind::DelimiterDepth),
                limits,
            ));
        }
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if !byte.is_ascii_whitespace() {
            previous = byte;
        }
        if depth > limits.max_delimiter_depth {
            return Some(issue(
                ProblemKind::SourceComplexityLimit,
                path,
                offset,
                "conservative raw delimiter depth limit reached",
                Some(AnalysisLimitKind::DelimiterDepth),
                limits,
            ));
        }
    }
    None
}

pub(crate) struct Parsed<T> {
    pub value: Option<T>,
    pub problems: Vec<AnalysisProblem>,
    pub nodes: usize,
}

pub(crate) fn parse<T, F>(
    path: &str,
    source: &str,
    limits: &AnalysisLimits,
    analyze: F,
) -> Result<Parsed<T>>
where
    T: Send + 'static,
    F: for<'a> FnOnce(&Semantic<'a>, &str) -> T + Send + 'static,
{
    validate_relative_path(path)?;
    anyhow::ensure!(
        is_source_path(path),
        "syntax analysis requires supported TypeScript source"
    );
    limits.validate()?;
    if let Some(problem) = preflight(path, source, limits) {
        return Ok(Parsed {
            value: None,
            problems: vec![problem],
            nodes: 0,
        });
    }
    let path = path.to_owned();
    let source = source.to_owned();
    let limits = limits.clone();
    let permit = ParserPermit::acquire();
    let context = (path.clone(), limits.clone());
    let handle = std::thread::Builder::new()
        .name("archguard-syntax".into())
        .stack_size(PARSER_STACK_BYTES)
        .spawn(move || {
            let _permit = permit;
            let allocator = Allocator::default();
            let parsed = Parser::new(
                &allocator,
                &source,
                SourceType::from_path(&path).expect("validated extension"),
            )
            .parse();
            let mut problems = vec![];
            let mut used = 0;
            for diagnostic in &parsed.diagnostics {
                let problem = issue(
                    ProblemKind::Parse,
                    &path,
                    diagnostic
                        .labels
                        .first()
                        .map_or(0, |label| label.offset() as usize),
                    &diagnostic.message,
                    None,
                    &limits,
                );
                if !crate::quality::load::push_evidence(&mut problems, problem, &mut used, &limits)
                {
                    problems.clear();
                    problems.push(issue(
                        ProblemKind::ReportLimit,
                        &path,
                        0,
                        "parser diagnostic evidence budget reached",
                        Some(AnalysisLimitKind::ReportBytes),
                        &limits,
                    ));
                    break;
                }
            }
            if !problems.is_empty() {
                return Parsed {
                    value: None,
                    problems,
                    nodes: 0,
                };
            }
            let built = SemanticBuilder::new_compiler()
                .with_build_nodes(true)
                .build(&parsed.program);
            for diagnostic in &built.diagnostics {
                if !crate::quality::load::push_evidence(
                    &mut problems,
                    issue(
                        ProblemKind::Binding,
                        &path,
                        0,
                        &diagnostic.message,
                        None,
                        &limits,
                    ),
                    &mut used,
                    &limits,
                ) {
                    problems.clear();
                    problems.push(issue(
                        ProblemKind::ReportLimit,
                        &path,
                        0,
                        "binding diagnostic evidence budget reached",
                        Some(AnalysisLimitKind::ReportBytes),
                        &limits,
                    ));
                    break;
                }
            }
            let nodes = built.semantic.nodes().len();
            if nodes > limits.max_nodes_per_file {
                problems.push(issue(
                    ProblemKind::AnalysisLimit,
                    &path,
                    0,
                    "syntax node limit reached",
                    Some(AnalysisLimitKind::NodesPerFile),
                    &limits,
                ));
            }
            if !problems.is_empty() {
                return Parsed {
                    value: None,
                    problems,
                    nodes,
                };
            }
            Parsed {
                value: Some(analyze(&built.semantic, &source)),
                problems,
                nodes,
            }
        });
    let (path, limits) = context;
    match handle {
        Ok(handle) => match handle.join() {
            Ok(parsed) => Ok(parsed),
            Err(_) => Ok(Parsed {
                value: None,
                problems: vec![issue(
                    ProblemKind::ParserRuntime,
                    &path,
                    0,
                    "syntax worker panicked",
                    None,
                    &limits,
                )],
                nodes: 0,
            }),
        },
        Err(error) => Ok(Parsed {
            value: None,
            problems: vec![issue(
                ProblemKind::ParserRuntime,
                &path,
                0,
                &error.to_string(),
                None,
                &limits,
            )],
            nodes: 0,
        }),
    }
}
