//! Accepted parser inputs and rejected resource limits, without timing claims.
use archguard::{
    mutator::{SyntaxValidation, validate_mutant},
    quality::{AnalysisLimits, ProblemKind},
};

fn accepted_and_rejected(name: &str, accepted: String, rejected: String) {
    let limits = AnalysisLimits::hard_maximum();
    let validation = validate_mutant("src/pressure.tsx", &accepted, &limits).unwrap();
    assert!(
        matches!(validation, SyntaxValidation::Valid),
        "accepted {name} input did not parse: {validation:?}"
    );
    println!("accepted {name}: {} source bytes", accepted.len());
    let SyntaxValidation::Incomplete { problems } =
        validate_mutant("src/pressure.tsx", &rejected, &limits).unwrap()
    else {
        panic!("excess {name} input must be rejected before parsing")
    };
    assert!(
        problems.iter().all(|problem| matches!(
            problem.kind,
            ProblemKind::SourceComplexityLimit | ProblemKind::AnalysisLimit
        )),
        "{name}: {problems:?}"
    );
    println!(
        "rejected {name}: {} source bytes, {:?}",
        rejected.len(),
        problems[0].limit
    );
}
fn function(expression: String) -> String {
    format!("function f(){{return {expression};}}")
}

#[test]
#[ignore = "run in the coordinated parser acceptance window, in debug and release"]
fn parser_accepts_the_hard_envelope_and_rejects_excess_input() {
    let hard = AnalysisLimits::hard_maximum();
    let depth = hard.max_delimiter_depth;
    accepted_and_rejected(
        "parentheses",
        format!("const value={}0{};", "(".repeat(depth), ")".repeat(depth)),
        format!(
            "const value={}0{};",
            "(".repeat(depth + 1),
            ")".repeat(depth + 1)
        ),
    );
    accepted_and_rejected(
        "objects",
        format!("const value={}0{};", "{x:".repeat(depth), "}".repeat(depth)),
        format!(
            "const value={}0{};",
            "{x:".repeat(depth + 1),
            "}".repeat(depth + 1)
        ),
    );
    accepted_and_rejected(
        "arrays",
        format!("const value={}0{};", "[".repeat(depth), "]".repeat(depth)),
        format!(
            "const value={}0{};",
            "[".repeat(depth + 1),
            "]".repeat(depth + 1)
        ),
    );
    accepted_and_rejected(
        "unary",
        function(format!("{}true", "!".repeat(depth))),
        function(format!("{}true", "!".repeat(depth + 1))),
    );
    accepted_and_rejected(
        "keyword unary",
        function(format!("{}0", "typeof ".repeat(depth))),
        function(format!("{}0", "typeof ".repeat(depth + 1))),
    );
    accepted_and_rejected(
        "constructors",
        function(format!("{}Object()", "new ".repeat(depth - 1))),
        function(format!("{}Object()", "new ".repeat(depth))),
    );
    accepted_and_rejected(
        "await",
        format!("async function f(){{return {}0;}}", "await ".repeat(depth)),
        format!(
            "async function f(){{return {}0;}}",
            "await ".repeat(depth + 1)
        ),
    );
    accepted_and_rejected(
        "assignment",
        format!("let a;{}", function(format!("{}0", "a=".repeat(depth)))),
        format!("let a;{}", function(format!("{}0", "a=".repeat(depth + 1)))),
    );
    accepted_and_rejected(
        "conditional",
        function(format!("{}0", "true?1:".repeat(depth / 2))),
        function(format!("{}0", "true?1:".repeat(depth / 2 + 1))),
    );
    accepted_and_rejected(
        "arrows",
        function(format!("{}0", "()=>".repeat((depth - 1) / 2))),
        function(format!("{}0", "()=>".repeat((depth - 1) / 2 + 1))),
    );
    accepted_and_rejected(
        "binary",
        function(format!("{}0", "0+".repeat(depth))),
        function(format!("{}0", "0+".repeat(depth + 1))),
    );
    accepted_and_rejected(
        "members",
        function(format!("base{}", ".value".repeat(depth))),
        function(format!("base{}", ".value".repeat(depth + 1))),
    );
    accepted_and_rejected(
        "calls",
        function(format!("callee{}", "()".repeat(depth))),
        function(format!("callee{}", "()".repeat(depth + 1))),
    );
    let jsx = (depth - 1) / 5;
    accepted_and_rejected(
        "JSX",
        format!("const value={}{};", "<a>".repeat(jsx), "</a>".repeat(jsx)),
        format!(
            "const value={}{};",
            "<a>".repeat(jsx + 1),
            "</a>".repeat(jsx + 1)
        ),
    );
    let generic = (depth - 1) / 2;
    accepted_and_rejected(
        "generic types",
        format!(
            "type Value={}number{};",
            "Array<".repeat(generic),
            ">".repeat(generic)
        ),
        format!(
            "type Value={}number{};",
            "Array<".repeat(generic + 1),
            ">".repeat(generic + 1)
        ),
    );
    let elements = (hard.max_raw_units - 5) / 2;
    accepted_and_rejected(
        "whole-file raw units",
        format!("const values=[{}0];", "0,".repeat(elements - 1)),
        format!("const values=[{}0];", "0,".repeat(elements)),
    );
    let prefix = "const value='";
    let suffix = "';";
    let bytes = hard.max_file_bytes - prefix.len() - suffix.len();
    accepted_and_rejected(
        "file bytes",
        format!("{prefix}{}{suffix}", "a".repeat(bytes)),
        format!("{prefix}{}{suffix}", "a".repeat(bytes + 1)),
    );
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "run in the coordinated parser acceptance window"]
fn repeated_cached_analysis_and_failures_release_file_descriptors() {
    use archguard::dryer::{DryerConfig, analyze_cached};
    let temp = tempfile::TempDir::new().unwrap();
    let source = temp.path().join("f.ts");
    let cache = temp.path().join("cache.json");
    std::fs::write(&source, "function f(x){return x+1}").unwrap();
    let config = DryerConfig::default();
    assert!(
        analyze_cached(temp.path(), &config, &cache)
            .unwrap()
            .complete
    );
    let descriptors = || std::fs::read_dir("/proc/self/fd").unwrap().count();
    let before = descriptors();
    for _ in 0..24 {
        assert!(
            analyze_cached(temp.path(), &config, &cache)
                .unwrap()
                .complete
        );
        std::fs::write(&source, "function f(x){return ; bad syntax}").unwrap();
        assert!(
            !analyze_cached(temp.path(), &config, &cache)
                .unwrap()
                .complete
        );
        std::fs::write(&source, "function f(x){return x+1}").unwrap();
        assert!(
            analyze_cached(temp.path(), &config, &cache)
                .unwrap()
                .complete
        );
    }
    assert_eq!(descriptors(), before);
    assert_eq!(
        std::fs::read_to_string(source).unwrap(),
        "function f(x){return x+1}"
    );
}
