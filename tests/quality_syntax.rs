use archguard::{
    dryer::{
        self, DryerConfig, LiteralValues, LocalIdentifiers, NormalizationOptions, PropertyNames,
    },
    mutator::{self, MutationOperator, MutationPlanConfig, SyntaxValidation},
    quality::{AnalysisLimitKind, AnalysisLimits, ProblemKind, SkipReason, SourceSelection},
};
use std::fs;
use tempfile::TempDir;

fn config() -> DryerConfig {
    DryerConfig {
        minimum_lines: 1,
        minimum_nodes: 1,
        similarity_threshold: 0.0,
        ..DryerConfig::default()
    }
}
fn compare(source: &str, options: NormalizationOptions) -> dryer::DryerReport {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("functions.tsx"), source).unwrap();
    let report = dryer::analyze(
        temp.path(),
        &DryerConfig {
            normalization: options,
            ..config()
        },
    )
    .unwrap();
    assert!(report.complete, "{:?}", report.problems);
    assert_eq!(report.functions.len(), 2, "{source}");
    assert_eq!(report.pairs.len(), 1);
    report
}
fn exact(source: &str) -> bool {
    compare(source, NormalizationOptions::default()).pairs[0].exact_normalized_match
}

#[test]
fn binding_relationships_renames_shadowing_and_captures_are_distinct() {
    assert!(exact(
        "function first(a,b){let c=a+b;return c+a} function second(x,y){let z=x+y;return z+x}"
    ));
    assert!(!exact(
        "function first(a,b){return a+a} function second(x,y){return x+y}"
    ));
    assert!(exact(
        "function first(a){let b=2;{let a=3;b=a}return a+b} function second(x){let y=4;{let x=5;y=x}return x+y}"
    ));
    assert!(!exact(
        "function first(a){let b=2;{let a=3;b=a}return a+b} function second(x){let y=4;{let z=5;y=x}return x+y}"
    ));
    assert!(exact(
        "let a=3,b=4; function first(x){return a+a+x} function second(y){return b+b+y}"
    ));
    assert!(!exact(
        "let a=3,b=4; function first(x){return a+a+x} function second(y){return a+b+y}"
    ));
    assert!(!exact(
        "function first(x){return globalOne+x} function second(y){return globalTwo+y}"
    ));
    let erased = compare(
        "function first(a,b){return a+a} function second(x,y){return x+y}",
        NormalizationOptions {
            local_identifiers: LocalIdentifiers::Erase,
            ..NormalizationOptions::default()
        },
    );
    assert!(erased.pairs[0].exact_normalized_match);
}

#[test]
fn called_operations_operators_order_and_optional_slots_are_preserved() {
    for source in [
        "function first(x){return fetch(x)} function second(x){return save(x)}",
        "function first(x){return x.map(v=>v+1)} function second(y){return y.filter(v=>v+1)}",
        "function first(x){return new Fetch(x)} function second(y){return new Save(y)}",
        "function first(x){return x+1} function second(y){return y-1}",
        "function first(x){return -x} function second(y){return +y}",
        "function first(x){return x++} function second(y){return ++y}",
        "function first(x){return x?.value} function second(y){return y.value}",
        "function first(x){return x?.()} function second(y){return y()}",
        "function first(x){for(x;;x){}} function second(y){for(;y;y){}}",
        "function first(x){return x&&1} function second(y){return 1&&y}",
        "function first(x){const [a,,b]=x;return a+b} function second(y){const [,a,b]=y;return a+b}",
        "async function first(x){return x} function second(y){return y}",
        "function* first(x){yield x} function second(y){return y}",
    ] {
        assert!(!exact(source), "{source}");
    }
    assert!(exact(
        "function first(x){return x.map(a=>a+1)} function second(y){return y.map(b=>b+3)}"
    ));
    let erased = compare(
        "function first(x){return x.map(v=>v+1)} function second(y){return y.filter(v=>v+1)}",
        NormalizationOptions {
            properties: PropertyNames::Erase,
            ..NormalizationOptions::default()
        },
    );
    assert!(!erased.pairs[0].exact_normalized_match);
}

#[test]
fn property_and_literal_modes_have_explicit_controls() {
    let source = "function first(x){return x.first+1} function second(y){return y.second+2}";
    assert!(!exact(source));
    let options = NormalizationOptions {
        properties: PropertyNames::Erase,
        ..NormalizationOptions::default()
    };
    assert!(compare(source, options).pairs[0].exact_normalized_match);
    assert!(!exact(
        "function first(x){return {\"one\":x}} function second(y){return {\"two\":y}}"
    ));
    assert!(!exact(
        "function first(x){return x['map']()} function second(y){return y['filter']()}"
    ));
    assert!(exact(
        "function first(x){return x+1} function second(y){return y+2}"
    ));
    assert!(
        !compare(
            "function first(x){return x+1} function second(y){return y+2}",
            NormalizationOptions {
                literals: LiteralValues::Value,
                ..NormalizationOptions::default()
            }
        )
        .pairs[0]
            .exact_normalized_match
    );
    assert!(!exact(
        "function first(x){return x+1} function second(y){return y+'text'}"
    ));
}

#[test]
fn selection_excludes_callbacks_overloads_and_nested_candidates() {
    let source = "declare function ambient():void; function overloaded(x:number):number; function overloaded(x:number){return x} const arrow=(x)=>{ const nested=(y)=>y; return [x].map(v=>nested(v)) }; const object={method(x){return x}, value:(x)=>x}; class C{method(x){return x} field=(x)=>x} [1].map(x=>x);";
    let inventory = dryer::extract(
        "src/functions.ts",
        source,
        &NormalizationOptions::default(),
        &AnalysisLimits::default(),
    )
    .unwrap();
    assert!(inventory.complete, "{:?}", inventory.problems);
    let names: Vec<_> = inventory
        .functions
        .iter()
        .map(|f| f.function.name.as_str())
        .collect();
    assert_eq!(names, ["overloaded", "arrow", "method", "method", "field"]);
    let same = compare(
        "function first(x){return [x].map(a=>a+1)} function second(y){return [y].map(b=>b+2)}",
        NormalizationOptions::default(),
    );
    assert!(same.pairs[0].exact_normalized_match);
}

#[test]
fn conservative_fallback_keeps_jsx_regex_and_unsupported_nodes_exact() {
    for source in [
        "function first(x){return <One a={x}/>} function second(y){return <Two a={y}/>}",
        "function first(x){return /abc/g.test(x)} function second(y){return /abc/i.test(y)}",
        "function first(x){return x as One} function second(y){return y as Two}",
        "function first(x){return tagOne`a${x}`} function second(y){return tagTwo`a${y}`}",
    ] {
        let report = compare(source, NormalizationOptions::default());
        assert!(!report.pairs[0].exact_normalized_match);
        assert!(report.functions.iter().all(|f| f.opaque_nodes > 0));
    }
    let same = compare(
        "function first(){return <One a={1} b={2}/>} function second(){return <One a={1} b={2}/>}",
        NormalizationOptions::default(),
    );
    assert!(same.pairs[0].exact_normalized_match);
    assert!(!exact(
        "function first(){return <One a={1} b={2}/>} function second(){return <One b={2} a={1}/>}"
    ));
}

#[test]
fn comparison_counts_and_resource_limits_preserve_partial_results() {
    let report = compare(
        "function first(x){return x+x} function second(y){return y+y+y}",
        NormalizationOptions::default(),
    );
    let pair = &report.pairs[0];
    assert!(!pair.exact_normalized_match);
    assert_ne!(pair.similarity.multiset, pair.similarity.set);
    assert!(pair.similarity.weighted < 1.0);
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("f.ts"),
        "function a(x){return x+1} function b(y){return y+2} function c(z){return z+3}",
    )
    .unwrap();
    let limited = DryerConfig {
        limits: AnalysisLimits {
            max_comparisons: 1,
            ..AnalysisLimits::default()
        },
        ..config()
    };
    let report = dryer::analyze(temp.path(), &limited).unwrap();
    assert!(!report.complete);
    assert_eq!(report.pairs.len(), 1);
    assert!(
        report
            .problems
            .iter()
            .any(|p| p.limit == Some(AnalysisLimitKind::Comparisons))
    );
    let restored = dryer::analyze(temp.path(), &config()).unwrap();
    assert!(restored.complete);
    assert_eq!(restored.pairs.len(), 3);
    let threshold = DryerConfig {
        similarity_threshold: 1.0,
        ..config()
    };
    assert_eq!(
        dryer::analyze(temp.path(), &threshold).unwrap().pairs.len(),
        3
    );
    let limited = DryerConfig {
        limits: AnalysisLimits {
            max_comparison_entries: 1,
            ..AnalysisLimits::default()
        },
        ..config()
    };
    assert!(
        dryer::analyze(temp.path(), &limited)
            .unwrap()
            .problems
            .iter()
            .any(|p| p.limit == Some(AnalysisLimitKind::ComparisonEntries))
    );
}

#[test]
fn mutation_sites_are_runtime_only_exact_and_cover_every_operator_group() {
    let source = "type Literal=true|0|1; const object={true:false,0:2,1:3}; function calculate(x:number,y:number){ let z=0x0; x++; --y; return x /*keep*/ <= y && x !== y || x + y - z * y / 1.0 > 0 ? true : false } const negative=-0;";
    let original = source.to_owned();
    let inventory = mutator::inventory("src/f.ts", source, &MutationPlanConfig::default()).unwrap();
    assert!(inventory.complete, "{:?}", inventory.problems);
    assert!(
        inventory
            .sites
            .iter()
            .all(|site| &source[site.location.start..site.location.end] == site.expected)
    );
    for operator in MutationOperator::ALL {
        assert!(
            inventory.sites.iter().any(|s| s.operator == operator),
            "{operator:?}"
        );
    }
    assert!(
        inventory
            .sites
            .iter()
            .all(|s| s.location.start > source.find("const object").unwrap())
    );
    assert!(
        !inventory
            .sites
            .iter()
            .any(|site| site.location.start == source.rfind('0').unwrap())
    );
    for site in &inventory.sites {
        let changed = mutator::apply_edit(source, site).unwrap();
        assert!(changed.contains("/*keep*/"));
        assert_eq!(
            &changed[..site.location.start],
            &source[..site.location.start]
        );
    }
    assert_eq!(source, original);
    let filtered = MutationPlanConfig {
        operators: vec![MutationOperator::Boolean],
        ..MutationPlanConfig::default()
    };
    assert!(
        mutator::inventory("src/f.ts", source, &filtered)
            .unwrap()
            .sites
            .iter()
            .all(|site| site.operator == MutationOperator::Boolean)
    );
}

#[test]
fn exact_utf8_crlf_edits_stale_inputs_and_syntax_classification_have_controls() {
    let source = "const emoji='😀';\r\nexport const f=()=>true;\r\n";
    let inventory = mutator::inventory("src/f.ts", source, &MutationPlanConfig::default()).unwrap();
    let site = &inventory.sites[0];
    assert_eq!(site.location.line, 2);
    assert_eq!(site.expected, "true");
    assert_eq!(
        mutator::apply_edit(source, site).unwrap(),
        source.replace("true", "false")
    );
    assert!(mutator::apply_edit(&source.replace("😀", "😁"), site).is_err());
    let mut malformed = site.clone();
    malformed.location.start += 1;
    assert!(mutator::apply_edit(source, &malformed).is_err());
    assert!(matches!(
        mutator::validate_mutant("src/f.ts", "const x = ;", &AnalysisLimits::default()).unwrap(),
        SyntaxValidation::InvalidSyntax { .. }
    ));
    assert!(matches!(
        mutator::validate_mutant("src/f.ts", "const x = 2;", &AnalysisLimits::default()).unwrap(),
        SyntaxValidation::Valid
    ));
    let awkward = "const f=()=>1..toString();";
    let site = mutator::inventory("src/f.ts", awkward, &MutationPlanConfig::default())
        .unwrap()
        .sites
        .remove(0);
    let changed = mutator::apply_edit(awkward, &site).unwrap();
    assert!(matches!(
        mutator::validate_mutant("src/f.ts", &changed, &AnalysisLimits::default()).unwrap(),
        SyntaxValidation::InvalidSyntax { .. }
    ));
}

#[test]
fn guards_are_typed_incomplete_and_a_longer_comparison_edit_is_not_invalid_syntax() {
    let source = "const f=()=>a<b";
    let mut limits = AnalysisLimits {
        max_delimiter_depth: 1,
        ..AnalysisLimits::default()
    };
    let minimum = (1..100)
        .find(|maximum| {
            limits.max_raw_units = *maximum;
            matches!(
                mutator::validate_mutant("src/f.ts", source, &limits).unwrap(),
                SyntaxValidation::Valid
            )
        })
        .unwrap();
    limits.max_raw_units = minimum;
    let site = mutator::inventory("src/f.ts", source, &MutationPlanConfig::default())
        .unwrap()
        .sites
        .remove(0);
    let changed = mutator::apply_edit(source, &site).unwrap();
    let SyntaxValidation::Incomplete { problems } =
        mutator::validate_mutant("src/f.ts", &changed, &limits).unwrap()
    else {
        panic!("lengthened edit must hit a resource guard")
    };
    assert_eq!(problems[0].kind, ProblemKind::SourceComplexityLimit);
    assert_eq!(problems[0].limit, Some(AnalysisLimitKind::RawUnits));
    limits.max_raw_units += 1;
    assert!(matches!(
        mutator::validate_mutant("src/f.ts", &changed, &limits).unwrap(),
        SyntaxValidation::Valid
    ));
    limits.max_file_bytes = changed.len() - 1;
    assert!(matches!(
        mutator::validate_mutant("src/f.ts", &changed, &limits).unwrap(),
        SyntaxValidation::Incomplete { .. }
    ));
    limits.max_file_bytes = changed.len();
    assert!(matches!(
        mutator::validate_mutant("src/f.ts", &changed, &limits).unwrap(),
        SyntaxValidation::Valid
    ));
    limits.max_delimiter_depth = 1;
    let inventory = dryer::extract(
        "src/f.ts",
        "const f=()=>((1));",
        &NormalizationOptions::default(),
        &limits,
    )
    .unwrap();
    assert!(!inventory.complete);
    assert!(
        inventory
            .problems
            .iter()
            .any(|p| p.limit == Some(AnalysisLimitKind::FileBytes)
                || p.limit == Some(AnalysisLimitKind::DelimiterDepth))
    );
}

#[test]
fn discovery_parse_failure_correction_and_source_preservation_are_explicit() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("bad.ts"), "const f=()=>;").unwrap();
    fs::write(temp.path().join("good.ts"), "const f=()=>true;").unwrap();
    fs::write(temp.path().join("ambient.d.ts"), "declare const f: true;").unwrap();
    fs::write(temp.path().join("outside.rs"), "fn f() {}").unwrap();
    let config = MutationPlanConfig::default();
    let failed = mutator::plan(temp.path(), &config).unwrap();
    assert!(!failed.complete);
    assert_eq!(failed.sites.len(), 1);
    assert!(failed.problems.iter().any(|p| p.kind == ProblemKind::Parse));
    assert!(
        failed
            .selection
            .skipped
            .iter()
            .any(|s| s.reason == SkipReason::DeclarationOnly)
    );
    fs::write(temp.path().join("bad.ts"), "const f=()=>false;").unwrap();
    let corrected = mutator::plan(temp.path(), &config).unwrap();
    assert!(corrected.complete);
    assert_eq!(corrected.sites.len(), 2);
    let before = fs::read(temp.path().join("good.ts")).unwrap();
    for site in &corrected.sites {
        let _ = mutator::apply_edit(
            &fs::read_to_string(temp.path().join(&site.location.file)).unwrap(),
            site,
        )
        .unwrap();
    }
    assert_eq!(fs::read(temp.path().join("good.ts")).unwrap(), before);
    let unsupported = MutationPlanConfig {
        selection: SourceSelection {
            include: vec!["**/*".into()],
            exclude: vec![],
        },
        ..config.clone()
    };
    assert!(!mutator::plan(temp.path(), &unsupported).unwrap().complete);
    let empty = MutationPlanConfig {
        selection: SourceSelection {
            include: vec!["missing/*.ts".into()],
            exclude: vec![],
        },
        ..config
    };
    assert!(!mutator::plan(temp.path(), &empty).unwrap().complete);
}

#[test]
fn cache_validates_bytes_discovery_options_corruption_and_source_destination() {
    let temp = TempDir::new().unwrap();
    let cache = temp.path().join("cache.json");
    let source = temp.path().join("f.ts");
    fs::write(
        &source,
        "function first(x){return x+1} function second(y){return y+2}",
    )
    .unwrap();
    let a = dryer::analyze_cached(temp.path(), &config(), &cache).unwrap();
    let b = dryer::analyze_cached(temp.path(), &config(), &cache).unwrap();
    assert!(a.complete && b.complete);
    assert_eq!(a.pairs, b.pairs);
    fs::write(
        &source,
        "function first(x){return x+1} function second(y){return y-2}",
    )
    .unwrap();
    let changed = dryer::analyze_cached(temp.path(), &config(), &cache).unwrap();
    assert!(!changed.pairs[0].exact_normalized_match);
    let fresh = dryer::analyze(temp.path(), &config()).unwrap();
    assert_eq!(fresh.pairs, changed.pairs);
    let value = DryerConfig {
        normalization: NormalizationOptions {
            literals: LiteralValues::Value,
            ..NormalizationOptions::default()
        },
        ..config()
    };
    assert_eq!(
        dryer::analyze_cached(temp.path(), &value, &cache)
            .unwrap()
            .pairs,
        dryer::analyze(temp.path(), &value).unwrap().pairs
    );
    fs::write(
        temp.path().join("extra.ts"),
        "function third(z){return z+4}",
    )
    .unwrap();
    assert_eq!(
        dryer::analyze_cached(temp.path(), &config(), &cache)
            .unwrap()
            .functions
            .len(),
        3
    );
    fs::remove_file(temp.path().join("extra.ts")).unwrap();
    assert_eq!(
        dryer::analyze_cached(temp.path(), &config(), &cache)
            .unwrap()
            .functions
            .len(),
        2
    );
    fs::write(&cache, "{corrupt}").unwrap();
    let rejected = dryer::analyze_cached(temp.path(), &config(), &cache).unwrap();
    assert!(!rejected.complete);
    assert!(
        rejected
            .problems
            .iter()
            .any(|p| p.message.contains("cache rejected"))
    );
    assert!(
        dryer::analyze_cached(temp.path(), &config(), &cache)
            .unwrap()
            .complete
    );
    let before = fs::read(&source).unwrap();
    assert!(dryer::analyze_cached(temp.path(), &config(), &source).is_err());
    assert_eq!(fs::read(&source).unwrap(), before);
}

#[test]
fn strict_dryer_configuration_and_bounded_names_remain_correctable() {
    for invalid in [
        serde_json::json!({}),
        serde_json::json!({"schemaVersion":1,"scoreGate":true}),
    ] {
        assert!(serde_json::from_value::<DryerConfig>(invalid).is_err());
    }
    let mut invalid = config();
    invalid.similarity_threshold = f64::NAN;
    assert!(invalid.validate().is_err());
    invalid.similarity_threshold = 0.0;
    invalid.validate().unwrap();
    invalid.normalization.normalization_version = 2;
    assert!(invalid.validate().is_err());
    invalid.normalization.normalization_version = 1;
    invalid.validate().unwrap();
    let limits = AnalysisLimits {
        max_name_bytes: 4,
        ..AnalysisLimits::default()
    };
    let inventory = dryer::extract(
        "src/f.ts",
        "function abcdef(x){return x+1} function abcdeg(y){return y+2}",
        &NormalizationOptions::default(),
        &limits,
    )
    .unwrap();
    assert!(inventory.complete);
    assert!(
        inventory
            .functions
            .iter()
            .all(|f| f.function.name == "abcd" && f.function.name_truncated)
    );
    let report = compare(
        "function abcdef(x){return x+1} function abcdeg(y){return y+2}",
        NormalizationOptions::default(),
    );
    assert!(report.pairs[0].exact_normalized_match);
}
