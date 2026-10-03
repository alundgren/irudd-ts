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
            .all(|site| source[site.location.start..site.location.end] == site.expected)
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
        max_delimiter_depth: 8,
        ..AnalysisLimits::default()
    };
    let minimum = (8..100)
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

#[test]
fn aggregate_reports_omit_evidence_before_collection_and_keep_json_valid() {
    let temp = TempDir::new().unwrap();
    let source = (0..20)
        .map(|i| format!("function name{i:03}{}(x){{return x+1}}\n", "a".repeat(220)))
        .collect::<String>();
    fs::write(temp.path().join("long.ts"), source).unwrap();
    let limited = DryerConfig {
        limits: AnalysisLimits {
            max_report_bytes: 64 * 1024,
            ..AnalysisLimits::default()
        },
        ..config()
    };
    let report = dryer::analyze(temp.path(), &limited).unwrap();
    assert!(!report.complete);
    assert!(report.omitted_evidence.pairs > 0);
    assert!(
        report
            .problems
            .iter()
            .any(|p| p.kind == ProblemKind::ReportLimit)
    );
    let encoded = serde_json::to_vec(&report).unwrap();
    assert!(encoded.len() <= limited.limits.max_report_bytes);
    serde_json::from_slice::<dryer::DryerReport>(&encoded).unwrap();
    let restored = dryer::analyze(temp.path(), &config()).unwrap();
    assert!(restored.complete);
    assert_eq!(restored.pairs.len(), 190);
    let short = TempDir::new().unwrap();
    fs::write(
        short.path().join("small.ts"),
        "function a(x){return x+1} function b(y){return y+2}",
    )
    .unwrap();
    assert!(dryer::analyze(short.path(), &limited).unwrap().complete);
    let mutants = MutationPlanConfig {
        limits: AnalysisLimits {
            max_sites: 1,
            ..AnalysisLimits::default()
        },
        ..MutationPlanConfig::default()
    };
    let partial = mutator::plan(short.path(), &mutants).unwrap();
    assert!(!partial.complete);
    assert_eq!(partial.sites.len(), 1);
    assert!(partial.omitted_evidence.sites > 0);
    let complete = mutator::plan(short.path(), &MutationPlanConfig::default()).unwrap();
    assert!(complete.complete);
    assert!(complete.sites.len() > partial.sites.len());
}

#[test]
fn truncated_display_names_do_not_change_exact_internal_operation_keys() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("f.ts"),"function abcdef(x){return operationFirst(x)} function abcdeg(y){return operationSecond(y)}").unwrap();
    let report = dryer::analyze(
        temp.path(),
        &DryerConfig {
            limits: AnalysisLimits {
                max_name_bytes: 4,
                ..AnalysisLimits::default()
            },
            ..config()
        },
    )
    .unwrap();
    assert!(report.complete);
    assert_eq!(report.pairs[0].left.name, report.pairs[0].right.name);
    assert!(report.pairs[0].left.name_truncated);
    assert!(!report.pairs[0].exact_normalized_match);
    assert_ne!(
        report.pairs[0].left.location.start,
        report.pairs[0].right.location.start
    );
    assert!(!exact(
        "const first={ [fetch()](){return true} }; const second={ [save()](){return true} };"
    ));
    assert!(!exact(
        "class One{#a=1;method(){return this.#a}} class Two{#b=1;method(){return this.#b}}"
    ));
}

#[test]
fn parallel_batches_produce_the_same_order_and_facts_as_one_worker() {
    let temp = TempDir::new().unwrap();
    for name in ["c", "a", "b"] {
        fs::write(
            temp.path().join(format!("{name}.ts")),
            format!("function {name}(x){{return x+1}}"),
        )
        .unwrap();
    }
    let single = dryer::analyze(temp.path(), &config()).unwrap();
    let parallel = DryerConfig {
        limits: AnalysisLimits {
            workers: 3,
            ..AnalysisLimits::default()
        },
        ..config()
    };
    let many = dryer::analyze(temp.path(), &parallel).unwrap();
    assert!(many.complete);
    assert_eq!(single.functions, many.functions);
    assert_eq!(single.pairs, many.pairs);
    let single = mutator::plan(temp.path(), &MutationPlanConfig::default()).unwrap();
    let many = mutator::plan(
        temp.path(),
        &MutationPlanConfig {
            limits: parallel.limits,
            ..MutationPlanConfig::default()
        },
    )
    .unwrap();
    assert!(many.complete);
    assert_eq!(single.sites, many.sites);
    assert_eq!(single.files, many.files);
}

#[cfg(unix)]
#[test]
fn selected_links_are_incomplete_and_cache_links_never_replace_input() {
    use std::os::unix::fs::symlink;
    let temp = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    fs::write(outside.path().join("f.ts"), "const f=()=>true").unwrap();
    symlink(outside.path().join("f.ts"), temp.path().join("f.ts")).unwrap();
    let linked = mutator::plan(temp.path(), &MutationPlanConfig::default()).unwrap();
    assert!(!linked.complete);
    assert!(
        linked
            .problems
            .iter()
            .any(|p| p.message.contains("symbolic link"))
    );
    fs::remove_file(temp.path().join("f.ts")).unwrap();
    fs::write(temp.path().join("f.ts"), "const f=()=>true").unwrap();
    assert!(
        mutator::plan(temp.path(), &MutationPlanConfig::default())
            .unwrap()
            .complete
    );
    let cache = temp.path().join("cache.json");
    symlink(outside.path().join("f.ts"), &cache).unwrap();
    let before = fs::read(outside.path().join("f.ts")).unwrap();
    assert!(
        !dryer::analyze_cached(temp.path(), &config(), &cache)
            .unwrap()
            .complete
    );
    assert_eq!(fs::read(outside.path().join("f.ts")).unwrap(), before);
}

#[test]
fn dogfood_existing_sdk_and_ts_examples_with_explicit_scope() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let selection = SourceSelection {
        include: vec!["sdk/**/*.ts".into(), "examples/**/*.ts".into()],
        exclude: vec![],
    };
    let dryer = DryerConfig {
        selection: selection.clone(),
        ..DryerConfig::default()
    };
    let analysis = dryer::analyze(root, &dryer).unwrap();
    assert!(analysis.complete, "{:?}", analysis.problems);
    assert!(!analysis.functions.is_empty());
    let plan = mutator::plan(
        root,
        &MutationPlanConfig {
            selection,
            ..MutationPlanConfig::default()
        },
    )
    .unwrap();
    assert!(plan.complete, "{:?}", plan.problems);
    assert!(!plan.sites.is_empty());
    assert!(plan.files.iter().all(|file| file.path.ends_with(".ts")));
    assert!(plan.files.iter().any(|file| file.path == "sdk/index.ts"));
    let original = fs::read(root.join("sdk/index.ts")).unwrap();
    for site in plan
        .sites
        .iter()
        .filter(|site| site.location.file == "sdk/index.ts")
        .take(3)
    {
        let edited = mutator::apply_edit(std::str::from_utf8(&original).unwrap(), site).unwrap();
        let _ = mutator::validate_mutant(&site.location.file, &edited, &plan.configuration.limits)
            .unwrap();
    }
    assert_eq!(fs::read(root.join("sdk/index.ts")).unwrap(), original);
}

#[test]
fn recursive_operator_guard_handles_punctuation_and_keyword_chains() {
    let limits = AnalysisLimits {
        max_delimiter_depth: 8,
        ..AnalysisLimits::default()
    };
    for expression in [
        "!!!!!!!!!!!true",
        "typeof typeof typeof typeof typeof typeof typeof typeof typeof 1",
        "0+0+0+0+0+0+0+0+0+0",
    ] {
        let source = format!("function f(){{return {expression};}}");
        let SyntaxValidation::Incomplete { problems } =
            mutator::validate_mutant("src/f.ts", &source, &limits).unwrap()
        else {
            panic!("unbounded recursive chain")
        };
        assert!(
            problems
                .iter()
                .any(|p| p.kind == ProblemKind::SourceComplexityLimit
                    && p.limit == Some(AnalysisLimitKind::DelimiterDepth))
        );
        assert!(matches!(
            mutator::validate_mutant("src/f.ts", &source, &AnalysisLimits::default()).unwrap(),
            SyntaxValidation::Valid
        ));
    }
    assert!(matches!(
        mutator::validate_mutant("src/f.ts", "function f(){return !!true;}", &limits).unwrap(),
        SyntaxValidation::Valid
    ));
}

#[cfg(unix)]
#[test]
fn selected_directory_links_do_not_claim_complete_descendant_coverage() {
    use std::os::unix::fs::symlink;
    let temp = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    fs::write(outside.path().join("f.ts"), "const f=()=>true").unwrap();
    fs::write(temp.path().join("known.ts"), "const known=()=>true").unwrap();
    symlink(outside.path(), temp.path().join("linked")).unwrap();
    let partial = mutator::plan(temp.path(), &MutationPlanConfig::default()).unwrap();
    assert!(!partial.complete);
    assert!(
        partial
            .problems
            .iter()
            .any(|p| p.kind == ProblemKind::Traversal)
    );
    let explicit = MutationPlanConfig {
        selection: SourceSelection {
            include: vec!["known.ts".into()],
            exclude: vec![],
        },
        ..MutationPlanConfig::default()
    };
    assert!(mutator::plan(temp.path(), &explicit).unwrap().complete);
    let excluded = MutationPlanConfig {
        selection: SourceSelection {
            exclude: vec!["linked/**".into()],
            ..SourceSelection::default()
        },
        ..MutationPlanConfig::default()
    };
    assert!(mutator::plan(temp.path(), &excluded).unwrap().complete);
}

#[test]
fn selected_source_byte_file_node_and_encoding_failures_are_correctable() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("a.ts"), "const a=()=>true").unwrap();
    fs::write(temp.path().join("b.ts"), "const b=()=>true").unwrap();
    for (limits, kind) in [
        (
            AnalysisLimits {
                max_file_bytes: 5,
                ..AnalysisLimits::default()
            },
            AnalysisLimitKind::FileBytes,
        ),
        (
            AnalysisLimits {
                max_file_bytes: 20,
                max_total_bytes: 20,
                ..AnalysisLimits::default()
            },
            AnalysisLimitKind::TotalBytes,
        ),
        (
            AnalysisLimits {
                max_files: 1,
                ..AnalysisLimits::default()
            },
            AnalysisLimitKind::Files,
        ),
        (
            AnalysisLimits {
                max_files: 1,
                max_discovery_entries: 2,
                ..AnalysisLimits::default()
            },
            AnalysisLimitKind::DiscoveryEntries,
        ),
        (
            AnalysisLimits {
                max_nodes_per_file: 1,
                ..AnalysisLimits::default()
            },
            AnalysisLimitKind::NodesPerFile,
        ),
    ] {
        let config = MutationPlanConfig {
            limits,
            ..MutationPlanConfig::default()
        };
        let limited = mutator::plan(temp.path(), &config).unwrap();
        assert!(!limited.complete);
        assert!(
            limited
                .problems
                .iter()
                .any(|problem| problem.limit == Some(kind)),
            "{kind:?}: {:?}",
            limited.problems
        );
        assert!(
            mutator::plan(temp.path(), &MutationPlanConfig::default())
                .unwrap()
                .complete
        );
    }
    fs::write(temp.path().join("a.ts"), [0xff, 0xfe]).unwrap();
    let invalid = mutator::plan(temp.path(), &MutationPlanConfig::default()).unwrap();
    assert!(!invalid.complete);
    assert!(
        invalid
            .problems
            .iter()
            .any(|problem| problem.kind == ProblemKind::InvalidUtf8)
    );
    fs::write(temp.path().join("a.ts"), "const a=()=>false").unwrap();
    let repaired = mutator::plan(temp.path(), &MutationPlanConfig::default()).unwrap();
    assert!(repaired.complete);
    assert_eq!(repaired.sites.len(), 2);
}

#[test]
fn candidate_limits_apply_before_size_filters_and_recovered_cache_trees_are_validated() {
    use sha2::{Digest, Sha256};
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("a.ts"), "const a=()=>true").unwrap();
    fs::write(temp.path().join("b.ts"), "const b=()=>true").unwrap();
    let limited = DryerConfig {
        limits: AnalysisLimits {
            max_candidates: 1,
            ..AnalysisLimits::default()
        },
        ..DryerConfig::default()
    };
    let partial = dryer::analyze(temp.path(), &limited).unwrap();
    assert!(!partial.complete);
    assert!(
        partial
            .problems
            .iter()
            .any(|problem| problem.limit == Some(AnalysisLimitKind::Candidates))
    );
    assert!(
        dryer::analyze(temp.path(), &DryerConfig::default())
            .unwrap()
            .complete
    );
    let cache = temp.path().join("cache.json");
    assert!(
        dryer::analyze_cached(temp.path(), &config(), &cache)
            .unwrap()
            .complete
    );
    let mut document: serde_json::Value =
        serde_json::from_slice(&fs::read(&cache).unwrap()).unwrap();
    document["payload"]["entries"][0]["functions"][0]["tree"][0]["children"] =
        serde_json::json!([999999]);
    document["sha256"] = serde_json::json!(
        Sha256::digest(serde_json::to_vec(&document["payload"]).unwrap())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    fs::write(&cache, serde_json::to_vec(&document).unwrap()).unwrap();
    let rejected = dryer::analyze_cached(temp.path(), &config(), &cache).unwrap();
    assert!(!rejected.complete);
    assert!(
        rejected
            .problems
            .iter()
            .any(|problem| problem.message.contains("postorder"))
    );
    assert!(
        dryer::analyze_cached(temp.path(), &config(), &cache)
            .unwrap()
            .complete
    );
    let mut document: serde_json::Value =
        serde_json::from_slice(&fs::read(&cache).unwrap()).unwrap();
    document["payload"]["version"] = serde_json::json!(2);
    document["sha256"] = serde_json::json!(
        Sha256::digest(serde_json::to_vec(&document["payload"]).unwrap())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    fs::write(&cache, serde_json::to_vec(&document).unwrap()).unwrap();
    let changed = dryer::analyze_cached(temp.path(), &config(), &cache).unwrap();
    assert!(changed.complete);
    assert_eq!(
        changed.pairs,
        dryer::analyze(temp.path(), &config()).unwrap().pairs
    );
}

#[test]
fn ambient_declarations_are_excluded_while_runtime_namespace_initializers_remain_present() {
    let source = "declare const ambient=true; declare enum Ambient{First=1} namespace Active{export const flag=false;export enum Values{First=0}}";
    let inventory =
        mutator::inventory("src/ambient.ts", source, &MutationPlanConfig::default()).unwrap();
    assert!(inventory.complete, "{:?}", inventory.problems);
    assert_eq!(inventory.sites.len(), 2);
    assert_eq!(
        inventory
            .sites
            .iter()
            .map(|site| site.expected.as_str())
            .collect::<Vec<_>>(),
        ["false", "0"]
    );
    let plain = mutator::inventory(
        "src/plain.ts",
        "const flag=true;enum Values{First=1}",
        &MutationPlanConfig::default(),
    )
    .unwrap();
    assert!(plain.complete);
    assert_eq!(plain.sites.len(), 2);
}

#[test]
fn mutation_count_and_byte_limits_keep_distinct_problem_categories() {
    let source = format!(
        "function {}(){{return [{}]}}",
        "n".repeat(220),
        "true,".repeat(40)
    );
    let config = MutationPlanConfig {
        limits: AnalysisLimits {
            max_report_bytes: 64 * 1024,
            ..AnalysisLimits::default()
        },
        ..MutationPlanConfig::default()
    };
    let limited = mutator::inventory("f.ts", &source, &config).unwrap();
    assert!(!limited.complete);
    assert!(
        limited
            .problems
            .iter()
            .any(|p| p.kind == ProblemKind::ReportLimit
                && p.limit == Some(AnalysisLimitKind::ReportBytes))
    );
    assert!(
        mutator::inventory("f.ts", &source, &MutationPlanConfig::default())
            .unwrap()
            .complete
    );
    assert!(
        mutator::inventory("f.ts", "const f=()=>true", &config)
            .unwrap()
            .complete
    );
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("a.ts"), "const a=()=>true").unwrap();
    fs::write(root.path().join("b.ts"), "const b=()=>false").unwrap();
    let limited = MutationPlanConfig {
        limits: AnalysisLimits {
            max_sites: 1,
            ..AnalysisLimits::default()
        },
        ..MutationPlanConfig::default()
    };
    let result = mutator::plan(root.path(), &limited).unwrap();
    assert!(!result.complete);
    assert_eq!(result.sites.len(), 1);
    assert!(
        result
            .problems
            .iter()
            .any(|p| p.kind == ProblemKind::AnalysisLimit
                && p.limit == Some(AnalysisLimitKind::Sites))
    );
    assert!(
        mutator::plan(root.path(), &MutationPlanConfig::default())
            .unwrap()
            .complete
    );
}

#[test]
fn configured_threshold_selects_unique_subtree_set_similarity() {
    let source = "function first(x){return x+x} function second(y){return y+y+y}";
    let evidence = compare(source, NormalizationOptions::default());
    let values = &evidence.pairs[0].similarity;
    assert!(values.set > values.weighted, "{values:?}");
    let threshold = (values.set + values.weighted) / 2.0;
    assert!(values.weighted < threshold && threshold < values.set);
    let root = TempDir::new().unwrap();
    fs::write(root.path().join("functions.ts"), source).unwrap();
    let selected = dryer::analyze(
        root.path(),
        &DryerConfig {
            similarity_threshold: threshold,
            ..config()
        },
    )
    .unwrap();
    assert!(selected.complete);
    assert_eq!(
        selected.pairs.len(),
        1,
        "set similarity clears the threshold even though weighted similarity does not"
    );
    assert_eq!(&selected.pairs[0].similarity, values);
    let rejected = dryer::analyze(
        root.path(),
        &DryerConfig {
            similarity_threshold: (values.set + 1.0) / 2.0,
            ..config()
        },
    )
    .unwrap();
    assert!(rejected.complete);
    assert!(rejected.pairs.is_empty());
    let lower = dryer::analyze(
        root.path(),
        &DryerConfig {
            similarity_threshold: values.weighted / 2.0,
            ..config()
        },
    )
    .unwrap();
    assert!(lower.complete);
    assert_eq!(lower.pairs.len(), 1);
}

#[test]
fn computed_call_names_preserve_literal_descendants_in_both_property_modes() {
    for properties in [PropertyNames::Preserve, PropertyNames::Erase] {
        let options = NormalizationOptions {
            properties,
            ..NormalizationOptions::default()
        };
        let different = compare(
            "function first(x){return x['ma'+'p']()} function second(y){return y['fil'+'ter']()}",
            options.clone(),
        );
        assert!(!different.pairs[0].exact_normalized_match);
        assert!(different.pairs[0].similarity.set < 1.0);
        let same = compare(
            "function first(x){return x['ma'+'p']()} function second(y){return y['ma'+'p']()}",
            options,
        );
        assert!(same.pairs[0].exact_normalized_match);
    }
}

#[test]
fn computed_property_keys_retain_runtime_mutations_while_static_keys_do_not() {
    let config = MutationPlanConfig::default();
    for source in [
        "const value={ [true ? 0 : 1]:2 };",
        "const { [true ? 0 : 1]:value }=input;",
        "class Value { [true ? 0 : 1](){return 2} }",
        "class Value { [true ? 0 : 1]=2 }",
    ] {
        let result = mutator::inventory("f.ts", source, &config).unwrap();
        assert!(result.complete, "{:?}", result.problems);
        assert_eq!(result.sites.len(), 3, "{source}");
        for site in &result.sites {
            let changed = mutator::apply_edit(source, site).unwrap();
            assert!(matches!(
                mutator::validate_mutant("f.ts", &changed, &config.limits).unwrap(),
                SyntaxValidation::Valid
            ));
        }
    }
    let static_keys = mutator::inventory(
        "f.ts",
        "const value={true:2,0:2,1:2}; class Value { true=2; 0(){return 2} }",
        &config,
    )
    .unwrap();
    assert!(static_keys.complete);
    assert!(static_keys.sites.is_empty());
}

#[test]
fn final_plan_discovery_detects_added_sources_and_ignores_excluded_changes() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let root = TempDir::new().unwrap();
    let original = "export const value=true;";
    fs::write(root.path().join("a.ts"), original).unwrap();
    let calls = AtomicUsize::new(0);
    let changed = mutator::plan_with_guard(root.path(), &MutationPlanConfig::default(), &|| {
        if calls.fetch_add(1, Ordering::Relaxed) == 7 {
            fs::write(root.path().join("added.ts"), "export const other=false;")?;
        }
        Ok(())
    })
    .unwrap();
    assert!(!changed.complete);
    assert!(
        changed
            .problems
            .iter()
            .any(|p| p.kind == ProblemKind::ChangedSource)
    );
    let stable = mutator::plan(root.path(), &MutationPlanConfig::default()).unwrap();
    assert!(stable.complete);
    assert_eq!(stable.files.len(), 2);
    assert_eq!(
        fs::read_to_string(root.path().join("a.ts")).unwrap(),
        original
    );
    fs::remove_file(root.path().join("added.ts")).unwrap();
    let calls = AtomicUsize::new(0);
    let config = MutationPlanConfig {
        selection: SourceSelection {
            exclude: vec!["added.ts".into()],
            ..SourceSelection::default()
        },
        ..MutationPlanConfig::default()
    };
    let excluded = mutator::plan_with_guard(root.path(), &config, &|| {
        if calls.fetch_add(1, Ordering::Relaxed) == 7 {
            fs::write(root.path().join("added.ts"), "export const other=false;")?;
        }
        Ok(())
    })
    .unwrap();
    assert!(excluded.complete, "{:?}", excluded.problems);
    assert_eq!(excluded.files.len(), 1);
}

#[test]
fn prospective_cache_paths_cannot_create_typescript_sources() {
    let root = TempDir::new().unwrap();
    let source = root.path().join("f.ts");
    let original = "function f(){return true}";
    fs::write(&source, original).unwrap();
    let invalid = root.path().join("new/cache.ts");
    assert!(dryer::analyze_cached(root.path(), &config(), &invalid).is_err());
    assert!(!invalid.exists());
    assert!(!root.path().join("new").exists());
    let corrected = root.path().join("new/cache.json");
    assert!(
        dryer::analyze_cached(root.path(), &config(), &corrected)
            .unwrap()
            .complete
    );
    assert!(corrected.is_file());
    assert!(
        dryer::analyze_cached(root.path(), &config(), &root.path().join("absent/../f.ts")).is_err()
    );
    assert!(!root.path().join("absent").exists());
    assert_eq!(fs::read_to_string(source).unwrap(), original);
}
