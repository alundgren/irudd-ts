use archguard::{
    mutator::{
        MutationInventory, MutationOperator, MutationPlan, MutationPlanConfig, SyntaxValidation,
    },
    quality::{
        AnalysisLimitKind, AnalysisLimits, AnalysisProblem, OmittedEvidence, ProblemKind,
        SelectionReport, SkipReason, SkippedSource, SourceFile, SourceLocation, SourceSelection,
        validate_report_size,
    },
};
use serde_json::{Value, json};
use std::{path::Path, process::Command};

fn fixture() -> MutationPlan {
    serde_json::from_str(include_str!("fixtures/quality/mutation-plan.json")).unwrap()
}

fn problem(kind: ProblemKind, limit: Option<AnalysisLimitKind>) -> AnalysisProblem {
    AnalysisProblem {
        kind,
        file: "src/flag.ts".into(),
        offset: 42,
        message: "source validation stopped".into(),
        message_truncated: false,
        limit,
    }
}

#[test]
fn invalid_configuration_is_correctable_without_weakening_negative_controls() {
    let mut config: MutationPlanConfig = serde_json::from_value(json!({
        "schemaVersion": 1, "limits": {"workers": 0}
    }))
    .unwrap();
    assert!(config.validate().is_err());
    config.limits.workers = 1;
    config.validate().unwrap();
    config.schema_version = 2;
    assert!(config.validate().is_err());
    config.schema_version = 1;
    config.validate().unwrap();
    config.operators.push(MutationOperator::Boolean);
    assert!(config.validate().is_err());
    config.operators = vec![MutationOperator::Boolean];
    config.validate().unwrap();
    config.operators.clear();
    assert!(config.validate().is_err());

    for value in [
        json!({}),
        json!({"schemaVersion":1,"implicitCommand":"npm test"}),
        json!({"schemaVersion":1,"limits":{"worker":1}}),
        json!({"schemaVersion":1,"selection":{"includes":["*.ts"]}}),
        json!({"schemaVersion":1,"operators":["unknown"]}),
    ] {
        assert!(serde_json::from_value::<MutationPlanConfig>(value).is_err());
    }
    assert_eq!(
        serde_json::from_value::<MutationPlanConfig>(json!({"schemaVersion":1})).unwrap(),
        MutationPlanConfig::default()
    );
}

#[test]
fn every_resource_ceiling_is_enforced_and_lower_valid_settings_remain_available() {
    let hard = serde_json::to_value(AnalysisLimits::hard_maximum()).unwrap();
    for (name, maximum) in hard.as_object().unwrap() {
        for invalid in [0, maximum.as_u64().unwrap() + 1] {
            let mut configured = serde_json::to_value(AnalysisLimits::default()).unwrap();
            configured[name] = json!(invalid);
            let limits: AnalysisLimits = serde_json::from_value(configured).unwrap();
            assert!(limits.validate().is_err(), "{name} accepted {invalid}");
        }
    }
    AnalysisLimits::hard_maximum().validate().unwrap();
    let mut limits = AnalysisLimits {
        max_raw_units: 256,
        max_delimiter_depth: 128,
        max_report_bytes: 64 * 1024,
        ..AnalysisLimits::default()
    };
    limits.validate().unwrap();
    limits.max_delimiter_depth = 257;
    assert!(limits.validate().is_err());
    limits.max_delimiter_depth = 128;
    limits.max_report_bytes -= 1;
    assert!(limits.validate().is_err());
    assert!(serde_json::from_value::<AnalysisLimits>(json!({"workers":-1})).is_err());
}

#[test]
fn canonical_source_locations_preserve_ts_variants_and_reject_unsafe_or_unsupported_paths() {
    let mut location = SourceLocation {
        file: "src/flag.ts".into(),
        start: 1,
        end: 5,
        line: 1,
        end_line: 1,
    };
    for invalid in [
        "/src/flag.ts",
        "../flag.ts",
        "src/../flag.ts",
        "src//flag.ts",
        "src\\flag.ts",
        "C:/flag.ts",
        "file:flag.ts",
        "src/flag\0.ts",
        "src/flag.jsx",
        "src/flag.rs",
        "src/flag.d.ts",
        "src/flag.d.mts",
        "src/flag.d.cts",
    ] {
        location.file = invalid.into();
        assert!(location.validate().is_err(), "accepted {invalid:?}");
    }
    for valid in [
        "src/flag.ts",
        "src/ui.tsx",
        "src/flag.mts",
        "src/flag.cts",
        "src/😀.ts",
    ] {
        location.file = valid.into();
        location.validate().unwrap();
    }
    location.end = location.start;
    assert!(location.validate().is_err());
    location.end += 1;
    location.line = 0;
    assert!(location.validate().is_err());
    location.line = 1;
    location.validate().unwrap();
}

#[test]
fn selection_and_configuration_failures_do_not_become_complete_empty_analysis() {
    let mut selection = SelectionReport {
        requested: SourceSelection::default(),
        selected: vec![],
        skipped: vec![],
        complete_within_selection: true,
    };
    assert!(selection.validate().is_err());
    selection.complete_within_selection = false;
    selection.validate().unwrap();
    selection.selected = fixture().files;
    selection.complete_within_selection = true;
    selection.validate().unwrap();
    selection.skipped.push(SkippedSource {
        path: "src/flag.rs".into(),
        reason: SkipReason::UnsupportedLanguage,
    });
    assert!(selection.validate().is_err());
    selection.complete_within_selection = false;
    selection.validate().unwrap();
    selection.skipped[0].path = selection.selected[0].path.clone();
    assert!(selection.validate().is_err());

    let mut patterns = SourceSelection::default();
    for invalid in ["", "../*.ts", "/src/*.ts", "src\\*.ts", "[unterminated"] {
        patterns.include = vec![invalid.into()];
        assert!(patterns.validate().is_err());
    }
    patterns.include = vec!["src/**/*.tsx".into()];
    patterns.exclude = vec!["src/generated/**".into()];
    patterns.validate().unwrap();
    patterns.include.push("x".repeat(17_000));
    assert!(patterns.validate().is_err());
}

#[test]
fn mutation_json_fixture_round_trips_exact_public_fields_and_versions() {
    let expected: Value =
        serde_json::from_str(include_str!("fixtures/quality/mutation-plan.json")).unwrap();
    let plan = fixture();
    plan.validate().unwrap();
    assert_eq!(serde_json::to_value(&plan).unwrap(), expected);
    assert_eq!(archguard::facts::SCHEMA_VERSION, 1);
    assert_eq!(archguard::semantic::SCHEMA_VERSION, 1);
    for field in ["schemaVersion", "operatorVersion"] {
        let mut value = expected.clone();
        value[field] = json!(2);
        assert!(
            serde_json::from_value::<MutationPlan>(value)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    let mut value = expected.clone();
    value["sites"][0]["owner"]["nameTruncated"] = json!("false");
    assert!(serde_json::from_value::<MutationPlan>(value).is_err());
    let mut value = expected;
    value["sites"][0]["extra"] = json!(true);
    assert!(serde_json::from_value::<MutationPlan>(value).is_err());
}

#[test]
fn node_and_rust_agree_on_utf8_source_hash_and_canonical_full_mutation_identity() {
    let fixture_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/quality/mutation-plan.json");
    let result = Command::new("node").args(["--input-type=module", "-e", r#"
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
const plan=JSON.parse(fs.readFileSync(process.argv[1],'utf8'));
const source=fs.readFileSync(path.join(path.dirname(process.argv[1]),'flag.ts'));
const site=plan.sites[0];
const hash=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
if(plan.files[0].bytes!==source.length || plan.files[0].sha256!==hash(source)) process.exit(1);
if(source.subarray(site.location.start,site.location.end).toString('utf8')!==site.expected) process.exit(2);
const identity=hash(JSON.stringify([plan.operatorVersion,site.location.file,site.sourceSha256,
site.location.start,site.location.end,site.expected,site.replacement]));
if(identity!==site.id || site.owner.nameTruncated!==false) process.exit(3);
process.stdout.write(identity);
"#]).arg(fixture_path).output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        String::from_utf8(result.stdout).unwrap(),
        fixture().sites[0].identity()
    );
}

#[test]
fn source_or_edit_changes_invalidate_sites_but_display_names_do_not_define_identity() {
    let plan = fixture();
    let mut site = plan.sites[0].clone();
    let original = site.id.clone();
    site.owner.as_mut().unwrap().name = "renamed evidence".into();
    site.owner.as_mut().unwrap().name_truncated = true;
    assert_eq!(site.identity(), original);
    site.validate().unwrap();
    site.replacement = "true".into();
    assert!(site.validate().is_err());
    site.replacement = "false".into();
    site.source_sha256 = "a".repeat(64);
    assert_ne!(site.identity(), original);
    assert!(site.validate().is_err());
    site.id = site.identity();
    site.validate().unwrap();
    let mut changed = plan.clone();
    changed.sites[0] = site;
    assert!(changed.validate().is_err());
    changed = plan.clone();
    changed.files[0].bytes += 1;
    assert!(changed.validate().is_err());
    changed = plan.clone();
    changed.sites.push(changed.sites[0].clone());
    assert!(changed.validate().is_err());
    plan.validate().unwrap();
}

#[test]
fn partial_inventory_and_resource_failure_are_distinct_from_syntax_invalidity() {
    let failure = problem(
        ProblemKind::SourceComplexityLimit,
        Some(AnalysisLimitKind::RawUnits),
    );
    let mut inventory = MutationInventory {
        sites: fixture().sites,
        complete: true,
        problems: vec![failure.clone()],
        omitted_evidence: OmittedEvidence::default(),
    };
    assert!(inventory.validate().is_err());
    inventory.complete = false;
    inventory.validate().unwrap();
    assert_eq!(inventory.sites.len(), 1);
    let invalid = SyntaxValidation::InvalidSyntax {
        diagnostics: vec![failure.clone()],
    };
    assert!(invalid.validate().is_err());
    let limited = SyntaxValidation::Incomplete {
        problems: vec![failure],
    };
    limited.validate().unwrap();
    let syntax = SyntaxValidation::InvalidSyntax {
        diagnostics: vec![problem(ProblemKind::Parse, None)],
    };
    syntax.validate().unwrap();
    assert_eq!(
        serde_json::to_value(&syntax).unwrap()["status"],
        "invalidSyntax"
    );
    assert_eq!(
        serde_json::to_value(&limited).unwrap()["status"],
        "incomplete"
    );
    SyntaxValidation::Valid.validate().unwrap();
    assert!(
        SyntaxValidation::InvalidSyntax {
            diagnostics: vec![]
        }
        .validate()
        .is_err()
    );
    assert!(
        SyntaxValidation::Incomplete { problems: vec![] }
            .validate()
            .is_err()
    );
    let mut plan = fixture();
    plan.omitted_evidence.sites = 1;
    assert!(plan.validate().is_err());
    plan.complete = false;
    plan.validate().unwrap();
}

#[test]
fn aggregate_encoded_bytes_bound_repeated_evidence_and_json_escaping() {
    let limits = AnalysisLimits {
        max_report_bytes: 64 * 1024,
        ..AnalysisLimits::default()
    };
    let large = vec!["\"\\\n".repeat(150); 100];
    assert!(validate_report_size(&large, &limits).is_err());
    validate_report_size(&large[..1], &limits).unwrap();
    let mut plan = fixture();
    plan.configuration.limits = limits;
    plan.selection.selected = (0..100)
        .map(|index| SourceFile {
            path: format!("src/{index:03}/{}.ts", "x".repeat(500)),
            bytes: 1,
            sha256: "a".repeat(64),
        })
        .collect();
    plan.files = plan.selection.selected.clone();
    plan.sites.clear();
    assert!(plan.validate().is_err());
    plan.configuration.limits.max_report_bytes = AnalysisLimits::default().max_report_bytes;
    plan.validate().unwrap();
}

#[test]
fn operator_groups_accept_only_the_advertised_replacement_directions() {
    for (operator, before, after) in [
        (MutationOperator::Comparison, "<=", "<"),
        (MutationOperator::Equality, "!==", "==="),
        (MutationOperator::Arithmetic, "*", "/"),
        (MutationOperator::Logical, "&&", "||"),
        (MutationOperator::Update, "++", "--"),
        (MutationOperator::Boolean, "false", "true"),
        (MutationOperator::ZeroOne, "0x0", "1"),
        (MutationOperator::ZeroOne, "1.0", "0"),
    ] {
        assert!(operator.accepts(before, after));
        assert!(!operator.accepts(before, before));
        assert!(!operator.accepts(before, "unknown"));
    }
    for invalid in ["2", "NaN", "-0", "0n", "true", "0 0"] {
        assert!(!MutationOperator::ZeroOne.accepts(invalid, "1"));
    }
}
