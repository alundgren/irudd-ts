use archguard::{cache::CachedAnalysis, config::Config, project, rules};
use std::{fs, path::Path};

fn copy(source: &Path, target: &Path) {
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let destination = target.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            fs::create_dir_all(&destination).unwrap();
            copy(&entry.path(), &destination);
        } else {
            fs::copy(entry.path(), destination).unwrap();
        }
    }
}

fn equivalent(root: &Path, config: &Config, cache: &Path) -> CachedAnalysis {
    let cached = project::analyze_cached(root, config, cache).unwrap();
    let fresh = project::analyze(root, config).unwrap();
    assert_eq!(
        serde_json::to_value(&cached.project).unwrap(),
        serde_json::to_value(&fresh).unwrap()
    );
    assert_eq!(
        serde_json::to_value(rules::check(&cached.project, config).unwrap()).unwrap(),
        serde_json::to_value(rules::check(&fresh, config).unwrap()).unwrap()
    );
    cached
}

#[test]
fn merged_barrel_edit_reruns_policy_for_reused_consumer() {
    let root = tempfile::tempdir().unwrap();
    let history = Path::new(env!("CARGO_MANIFEST_DIR")).join("research/t3code/history/14389");
    copy(&history.join("before"), root.path());
    let config: Config =
        serde_json::from_slice(&fs::read(root.path().join("archguard.json")).unwrap()).unwrap();
    let cache = root.path().join("cache.json");
    let before = equivalent(root.path(), &config, &cache);
    assert_eq!(rules::check(&before.project, &config).unwrap().len(), 1);
    let barrel = "packages/client-runtime/src/connection/index.ts";
    fs::copy(history.join("fixed").join(barrel), root.path().join(barrel)).unwrap();
    let fixed = equivalent(root.path(), &config, &cache);
    assert!(fixed.project.problems.is_empty());
    assert!(rules::check(&fixed.project, &config).unwrap().is_empty());
    assert_eq!(fixed.cache.parsed_files, 1);
    assert!(fixed.cache.reused_edges > 0);
    assert!(fixed.cache.resolved_edges > 0);
}

#[test]
fn merged_type_extraction_addition_missing_target_and_reverse_deletion() {
    let history = Path::new(env!("CARGO_MANIFEST_DIR")).join("research/t3code/history/13151");
    for profile in ["", ".android", ".ios"] {
        let root = tempfile::tempdir().unwrap();
        copy(&history.join("before"), root.path());
        let config: Config = serde_json::from_slice(
            &fs::read(root.path().join(format!("archguard{profile}.json"))).unwrap(),
        )
        .unwrap();
        let cache = root.path().join("cache.json");
        let before = equivalent(root.path(), &config, &cache);
        assert_eq!(rules::check(&before.project, &config).unwrap().len(), 1);
        assert!(equivalent(root.path(), &config, &cache).cache.reused_edges > 0);
        let generic = "apps/mobile/src/components/FilePreview.tsx";
        fs::copy(
            history.join("fixed").join(generic),
            root.path().join(generic),
        )
        .unwrap();
        let missing = equivalent(root.path(), &config, &cache);
        assert!(!missing.project.problems.is_empty());
        assert!(!missing.cache.published);
        let types = "apps/mobile/src/components/FilePreviewModal.types.ts";
        fs::copy(history.join("fixed").join(types), root.path().join(types)).unwrap();
        let corrected = equivalent(root.path(), &config, &cache);
        assert!(corrected.project.problems.is_empty());
        assert_eq!(corrected.cache.reused_edges, 0);
        for path in [
            "apps/mobile/src/components/FilePreview.ios.tsx",
            "apps/mobile/src/components/FilePreviewModal.tsx",
        ] {
            fs::copy(history.join("fixed").join(path), root.path().join(path)).unwrap();
            equivalent(root.path(), &config, &cache);
        }
        let fixed = equivalent(root.path(), &config, &cache);
        assert!(rules::check(&fixed.project, &config).unwrap().is_empty());
        copy(&history.join("before"), root.path());
        fs::remove_file(root.path().join(types)).unwrap();
        let reversed = equivalent(root.path(), &config, &cache);
        assert!(reversed.project.problems.is_empty());
        assert_eq!(rules::check(&reversed.project, &config).unwrap().len(), 1);
        assert_eq!(reversed.cache.reused_edges, 0);
    }
}
