use crate::{
    config::{Config, Matcher},
    facts::*,
    rust, typescript,
};
use anyhow::{Context, Result, bail};
use oxc_resolver::{FileMetadata, FileSystem, FileSystemOs, ResolveError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

const CACHE_VERSION: u32 = 1;
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
static EXECUTABLE_DIGEST: OnceLock<std::result::Result<String, String>> = OnceLock::new();

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheStats {
    pub parsed_files: usize,
    pub reused_files: usize,
    pub resolved_edges: usize,
    pub reused_edges: usize,
    pub resolver_inputs: usize,
    pub published: bool,
}

pub struct CachedAnalysis {
    pub project: ProjectFacts,
    pub cache: CacheStats,
}

#[derive(Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Identity {
    root: PathBuf,
    facts_version: u32,
    executable: String,
    dependency_lock: String,
    config: serde_json::Value,
    parser: String,
    resolver: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceEntry {
    digest: String,
    syntax: FileFacts,
    resolved: FileFacts,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    identity: Identity,
    sources: BTreeMap<String, SourceEntry>,
    packages: BTreeMap<String, String>,
    available: BTreeSet<String>,
    observations: Vec<Observation>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u32,
    checksum: String,
    snapshot: Snapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
enum Operation {
    Read,
    ReadString,
    Metadata,
    SymlinkMetadata,
    ReadLink,
    Canonicalize,
    HostMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Observed {
    Contents(String),
    Kind {
        file: bool,
        directory: bool,
        link: bool,
    },
    Path(PathBuf),
    Error(String),
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Observation {
    operation: Operation,
    path: PathBuf,
    value: Observed,
}

#[derive(Default)]
struct Observations {
    values: BTreeMap<(Operation, PathBuf), Observed>,
    changed: bool,
}

#[derive(Clone, Default)]
pub(crate) struct RecordingFileSystem {
    observations: Option<Arc<Mutex<Observations>>>,
}

impl RecordingFileSystem {
    fn recording() -> Self {
        Self {
            observations: Some(Arc::default()),
        }
    }

    fn record(&self, operation: Operation, path: &Path, value: impl FnOnce() -> Observed) {
        if let Some(observations) = &self.observations {
            let value = value();
            let mut observations = observations.lock().unwrap();
            let previous = observations
                .values
                .insert((operation, path.into()), value.clone());
            observations.changed |= previous.is_some_and(|previous| previous != value);
        }
    }

    pub(crate) fn host_is_file(&self, path: &Path) -> bool {
        let result = fs::metadata(path);
        self.record(Operation::HostMetadata, path, || {
            observe(&result, |m| kind(&FileMetadata::from(m.clone())))
        });
        result.is_ok_and(|m| m.is_file())
    }

    fn collect(&self) -> (Vec<Observation>, bool) {
        let observations = self.observations.as_ref().unwrap().lock().unwrap();
        (
            observations
                .values
                .iter()
                .map(|((operation, path), value)| Observation {
                    operation: *operation,
                    path: path.clone(),
                    value: value.clone(),
                })
                .collect(),
            observations.changed,
        )
    }
}

pub(crate) fn digest(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        encoded.push(char::from(b"0123456789abcdef"[usize::from(byte >> 4)]));
        encoded.push(char::from(b"0123456789abcdef"[usize::from(byte & 15)]));
    }
    encoded
}

fn kind(metadata: &FileMetadata) -> Observed {
    Observed::Kind {
        file: metadata.is_file(),
        directory: metadata.is_dir(),
        link: metadata.is_symlink(),
    }
}

fn executable_digest() -> Result<String> {
    EXECUTABLE_DIGEST
        .get_or_init(|| {
            std::env::current_exe()
                .and_then(fs::read)
                .map(|bytes| digest(&bytes))
                .map_err(|error| error.to_string())
        })
        .clone()
        .map_err(anyhow::Error::msg)
}

fn observe<T, E: std::fmt::Debug>(
    result: &std::result::Result<T, E>,
    value: impl FnOnce(&T) -> Observed,
) -> Observed {
    match result {
        Ok(result) => value(result),
        Err(error) => Observed::Error(format!("{error:?}")),
    }
}

impl FileSystem for RecordingFileSystem {
    fn new() -> Self {
        Self::default()
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        let result = FileSystemOs::new().read(path);
        self.record(Operation::Read, path, || {
            observe(&result, |bytes| Observed::Contents(digest(bytes)))
        });
        result
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        let result = FileSystemOs::new().read_to_string(path);
        self.record(Operation::ReadString, path, || {
            observe(&result, |text| Observed::Contents(digest(text.as_bytes())))
        });
        result
    }

    fn metadata(&self, path: &Path) -> io::Result<FileMetadata> {
        let result = FileSystemOs::new().metadata(path);
        self.record(Operation::Metadata, path, || observe(&result, kind));
        result
    }

    fn symlink_metadata(&self, path: &Path) -> io::Result<FileMetadata> {
        let result = FileSystemOs::new().symlink_metadata(path);
        self.record(Operation::SymlinkMetadata, path, || observe(&result, kind));
        result
    }

    fn read_link(&self, path: &Path) -> std::result::Result<PathBuf, ResolveError> {
        let result = FileSystemOs::new().read_link(path);
        self.record(Operation::ReadLink, path, || {
            observe(&result, |path| Observed::Path(path.clone()))
        });
        result
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        let result = FileSystemOs::new().canonicalize(path);
        self.record(Operation::Canonicalize, path, || {
            observe(&result, |path| Observed::Path(path.clone()))
        });
        result
    }
}

impl Observation {
    fn valid(&self) -> bool {
        let fs = RecordingFileSystem::default();
        let value = match self.operation {
            Operation::Read => observe(&fs.read(&self.path), |bytes| {
                Observed::Contents(digest(bytes))
            }),
            Operation::ReadString => observe(&fs.read_to_string(&self.path), |text| {
                Observed::Contents(digest(text.as_bytes()))
            }),
            Operation::Metadata => observe(&fs.metadata(&self.path), kind),
            Operation::SymlinkMetadata => observe(&fs.symlink_metadata(&self.path), kind),
            Operation::HostMetadata => observe(&std::fs::metadata(&self.path), |m| {
                kind(&FileMetadata::from(m.clone()))
            }),
            Operation::ReadLink => observe(&fs.read_link(&self.path), |path| {
                Observed::Path(path.clone())
            }),
            Operation::Canonicalize => observe(&fs.canonicalize(&self.path), |path| {
                Observed::Path(path.clone())
            }),
        };
        value == self.value
    }
}

pub(crate) struct CacheRun {
    path: PathBuf,
    identity: Identity,
    previous: Option<Snapshot>,
    sources: BTreeMap<String, SourceEntry>,
    packages: BTreeMap<String, String>,
    graph_valid: bool,
    fs: RecordingFileSystem,
    pub stats: CacheStats,
    errors: Vec<String>,
}

impl CacheRun {
    pub fn begin(root: &Path, config: &Config, path: &Path) -> Result<Self> {
        let mut path = if path.is_absolute() {
            path.to_owned()
        } else {
            std::env::current_dir()?.join(path)
        };
        let identity = Identity {
            root: root.canonicalize()?,
            facts_version: SCHEMA_VERSION,
            executable: executable_digest()?,
            dependency_lock: digest(include_bytes!("../Cargo.lock")),
            config: serde_json::to_value(config)?,
            parser: "oxc-0.152.0;syn-2".into(),
            resolver: "oxc-resolver-11.24.3".into(),
        };
        let destination = path
            .parent()
            .and_then(|parent| parent.canonicalize().ok())
            .and_then(|parent| path.file_name().map(|name| parent.join(name)))
            .unwrap_or_else(|| path.clone());
        if let Ok(relative) = destination.strip_prefix(&identity.root) {
            let relative = relative.to_string_lossy().replace('\\', "/");
            let extension = destination
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default();
            let source = matches!(
                extension,
                "ts" | "tsx" | "mts" | "cts" | "js" | "jsx" | "mjs" | "cjs" | "rs"
            ) && Matcher::new(&config.include)?.matches(&relative);
            if !Matcher::new(&config.exclude)?.matches(&relative)
                && (source || Matcher::new(&config.package_manifests)?.matches(&relative))
            {
                bail!("cache destination is a selected source or package manifest: {relative}");
            }
        }
        path = destination;
        let mut errors = vec![];
        let previous = match fs::read(&path) {
            Ok(bytes) => match load(&bytes, &identity) {
                Ok(Some(snapshot)) if snapshot.identity == identity => Some(snapshot),
                Ok(_) => None,
                Err(error) => {
                    errors.push(format!("cache read: {error:#}"));
                    None
                }
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => {
                errors.push(format!("cache read: {error}"));
                None
            }
        };
        Ok(Self {
            path,
            identity,
            previous,
            sources: BTreeMap::new(),
            packages: BTreeMap::new(),
            graph_valid: false,
            fs: RecordingFileSystem::recording(),
            stats: CacheStats::default(),
            errors,
        })
    }

    pub fn filesystem(&self) -> RecordingFileSystem {
        self.fs.clone()
    }

    pub fn package_input(&mut self, path: &str, source: &str) {
        self.packages.insert(path.into(), digest(source.as_bytes()));
    }

    pub fn parse(&mut self, path: &str, source: &str, extension: &str) -> Result<FileFacts> {
        let digest = digest(source.as_bytes());
        let syntax = if let Some(entry) = self
            .previous
            .as_ref()
            .and_then(|snapshot| snapshot.sources.get(path))
            .filter(|entry| entry.digest == digest)
        {
            self.stats.reused_files += 1;
            entry.syntax.clone()
        } else {
            self.stats.parsed_files += 1;
            if extension == "rs" {
                rust::parse(path, source)?
            } else {
                typescript::parse(path, source)?
            }
        };
        self.sources.insert(
            path.into(),
            SourceEntry {
                digest,
                syntax: syntax.clone(),
                resolved: syntax.clone(),
            },
        );
        Ok(syntax)
    }

    pub fn prepare_graph(&mut self, available: &BTreeSet<String>) {
        self.graph_valid = self.previous.as_ref().is_some_and(|snapshot| {
            snapshot.available == *available
                && snapshot.packages == self.packages
                && snapshot.observations.iter().all(Observation::valid)
        });
    }

    pub fn reuse_graph(&mut self, file: &mut FileFacts) -> bool {
        if self.graph_valid {
            let current = &self.sources[&file.path];
            if let Some(entry) = self
                .previous
                .as_ref()
                .and_then(|snapshot| snapshot.sources.get(&file.path))
                .filter(|entry| entry.digest == current.digest)
            {
                *file = entry.resolved.clone();
                self.stats.reused_edges += file.imports.len();
                return true;
            }
        }
        self.stats.resolved_edges += file.imports.len();
        false
    }

    pub fn finish(
        mut self,
        project: &mut ProjectFacts,
        current_inputs: impl FnOnce() -> Result<BTreeMap<String, String>>,
    ) -> CacheStats {
        let (fresh, mut changed) = self.fs.collect();
        let mut observations = BTreeMap::new();
        if self.graph_valid {
            for observation in &self.previous.as_ref().unwrap().observations {
                observations.insert(
                    (observation.operation, observation.path.clone()),
                    observation.clone(),
                );
            }
        }
        for observation in fresh {
            changed |= observations
                .get(&(observation.operation, observation.path.clone()))
                .is_some_and(|old: &Observation| old.value != observation.value);
            observations.insert(
                (observation.operation, observation.path.clone()),
                observation,
            );
        }
        let observations: Vec<_> = observations.into_values().collect();
        self.stats.resolver_inputs = observations.len();
        if project.problems.is_empty() {
            let expected: BTreeMap<_, _> = self
                .packages
                .iter()
                .map(|(path, digest)| (path.clone(), digest.clone()))
                .chain(
                    self.sources
                        .iter()
                        .map(|(path, entry)| (path.clone(), entry.digest.clone())),
                )
                .collect();
            match current_inputs() {
                Ok(inputs)
                    if inputs == expected
                        && !changed
                        && observations.iter().all(Observation::valid) =>
                {
                    let canonical_destination = self
                        .path
                        .canonicalize()
                        .unwrap_or_else(|_| self.path.clone());
                    let destination_used = observations.iter().any(|observation| observation.path == self.path
                        || matches!(&observation.value, Observed::Path(path) if path == &canonical_destination));
                    if destination_used {
                        self.errors.push(
                            "cache destination is a resolver input; graph was not published".into(),
                        );
                    } else if !self.graph_valid
                        || self.stats.parsed_files > 0
                        || self.stats.resolved_edges > 0
                    {
                        for file in &project.files {
                            self.sources.get_mut(&file.path).unwrap().resolved = file.clone();
                        }
                        let snapshot = Snapshot {
                            identity: self.identity,
                            available: self.sources.keys().cloned().collect(),
                            sources: self.sources,
                            packages: self.packages,
                            observations,
                        };
                        match publish(&self.path, snapshot) {
                            Ok(()) => self.stats.published = true,
                            Err(error) => self.errors.push(format!("cache write: {error:#}")),
                        }
                    }
                }
                Ok(_) => self
                    .errors
                    .push("cache inputs changed during analysis; graph was not published".into()),
                Err(error) => self
                    .errors
                    .push(format!("cache input validation: {error:#}")),
            }
        }
        project
            .problems
            .extend(self.errors.into_iter().map(|message| Problem {
                file: ".".into(),
                offset: 0,
                message,
            }));
        project
            .problems
            .sort_by(|a, b| (&a.file, a.offset, &a.message).cmp(&(&b.file, b.offset, &b.message)));
        self.stats
    }
}

fn load(bytes: &[u8], identity: &Identity) -> Result<Option<Snapshot>> {
    let header: serde_json::Value = serde_json::from_slice(bytes).context("invalid cache JSON")?;
    if header
        .get("version")
        .and_then(|v| v.as_u64())
        .is_some_and(|v| v != u64::from(CACHE_VERSION))
    {
        return Ok(None);
    }
    let snapshot = header.get("snapshot").context("missing cache snapshot")?;
    if header.get("checksum").and_then(|v| v.as_str())
        != Some(digest(&serde_json::to_vec(snapshot)?).as_str())
    {
        bail!("cache checksum mismatch");
    }
    let saved_identity: Identity = serde_json::from_value(
        snapshot
            .get("identity")
            .context("missing cache identity")?
            .clone(),
    )?;
    if saved_identity != *identity {
        return Ok(None);
    }
    let envelope: Envelope = serde_json::from_value(header).context("invalid cache envelope")?;
    for (path, entry) in &envelope.snapshot.sources {
        if path != &entry.syntax.path || path != &entry.resolved.path {
            bail!("cache file identity mismatch");
        }
    }
    if envelope.snapshot.available != envelope.snapshot.sources.keys().cloned().collect() {
        bail!("cache source inventory mismatch");
    }
    Ok(Some(envelope.snapshot))
}

fn publish(path: &Path, snapshot: Snapshot) -> Result<()> {
    let parent = path.parent().context("cache path has no parent")?;
    fs::create_dir_all(parent)?;
    let envelope = Envelope {
        version: CACHE_VERSION,
        checksum: digest(&serde_json::to_vec(&snapshot)?),
        snapshot,
    };
    let temporary = parent.join(format!(
        ".archguard-cache-{}-{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(&serde_json::to_vec(&envelope)?)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use crate::{
        cache::{CacheRun, Envelope, publish},
        config::Config,
    };
    use oxc_resolver::FileSystem;
    use std::fs;

    #[test]
    fn executing_binary_identity_change_discards_a_valid_entry() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("a.ts"), "export {};").unwrap();
        let config: Config = serde_json::from_str(r#"{"schemaVersion":1}"#).unwrap();
        let path = root.path().join("cache.json");
        crate::project::analyze_cached(root.path(), &config, &path).unwrap();
        let mut envelope: Envelope = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        envelope.snapshot.identity.executable = "another executable".into();
        publish(&path, envelope.snapshot).unwrap();
        let analysis = crate::project::analyze_cached(root.path(), &config, &path).unwrap();
        assert!(analysis.project.problems.is_empty());
        assert_eq!(analysis.cache.reused_files, 0);
        assert_eq!(analysis.cache.parsed_files, 1);
    }

    #[test]
    fn observations_changed_after_reuse_cannot_be_published() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("a.ts"), "import './b.ts';").unwrap();
        fs::write(root.path().join("b.ts"), "export {};").unwrap();
        let config: Config = serde_json::from_str(r#"{"schemaVersion":1}"#).unwrap();
        let path = root.path().join("cache.json");
        let mut project = crate::project::analyze_cached(root.path(), &config, &path)
            .unwrap()
            .project;
        let mut run = CacheRun::begin(root.path(), &config, &path).unwrap();
        for file in &project.files {
            run.parse(
                &file.path,
                &fs::read_to_string(root.path().join(&file.path)).unwrap(),
                "ts",
            )
            .unwrap();
        }
        run.prepare_graph(&project.files.iter().map(|file| file.path.clone()).collect());
        assert!(run.reuse_graph(&mut project.files[0]));
        let inputs = run
            .sources
            .iter()
            .map(|(path, entry)| (path.clone(), entry.digest.clone()))
            .collect();
        fs::remove_file(root.path().join("b.ts")).unwrap();
        assert!(
            run.filesystem()
                .metadata(&root.path().join("b.ts"))
                .is_err()
        );
        let stats = run.finish(&mut project, || Ok(inputs));
        assert!(!stats.published);
        assert!(!project.problems.is_empty());
        assert!(
            project.problems[0]
                .message
                .contains("changed during analysis")
        );
    }
}
