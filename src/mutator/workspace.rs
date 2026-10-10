use crate::mutator::{
    config::{ExecutionConfig, ExecutionLimits},
    storage,
};
use crate::{config::Matcher, quality::validate_relative_path};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::os::fd::IntoRawFd;
#[cfg(unix)]
use std::os::unix::{
    ffi::OsStrExt,
    fs::{MetadataExt, OpenOptionsExt, PermissionsExt, symlink},
};
use std::{
    collections::{BTreeSet, VecDeque},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Debug)]
pub(crate) struct WorkspaceLimit(pub &'static str);
impl std::fmt::Display for WorkspaceLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for WorkspaceLimit {}
fn limit(message: &'static str) -> anyhow::Error {
    WorkspaceLimit(message).into()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Entry {
    pub path: String,
    pub kind: EntryKind,
    pub bytes: u64,
    pub mode: u32,
    pub digest: String,
    pub link: Option<String>,
    #[serde(skip)]
    pub source: PathBuf,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum EntryKind {
    Directory,
    File,
    Link,
}
#[derive(Clone)]
pub(crate) struct Inputs {
    requested_root: PathBuf,
    pub root: PathBuf,
    pub roots: Vec<(PathBuf, String)>,
    pub entries: Vec<Entry>,
    pub bytes: u64,
    pub root_mode: u32,
    pub resolutions: Vec<ResolutionEvidence>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResolutionEvidence {
    pub path: PathBuf,
    pub mode: u32,
    pub kind: EntryKind,
    pub link: Option<PathBuf>,
}
pub(crate) fn resolve_evidence(
    path: &Path,
    maximum: u64,
    guard: &impl Fn() -> Result<()>,
) -> Result<(PathBuf, Vec<ResolutionEvidence>)> {
    if !path.is_absolute() {
        bail!("execution input path must be absolute");
    }
    let mut current = path_root(path);
    let mut pending: VecDeque<_> = path
        .components()
        .filter(|part| !matches!(part, Component::RootDir | Component::Prefix(_)))
        .map(|part| part.as_os_str().to_owned())
        .collect();
    let mut evidence = Vec::new();
    let mut links = 0usize;
    let mut used = 0u64;
    while let Some(part) = pending.pop_front() {
        guard()?;
        if part == "." {
            continue;
        }
        if part == ".." {
            current.pop();
            continue;
        }
        let candidate = current.join(part);
        let metadata = fs::symlink_metadata(&candidate)?;
        let kind = if is_link(&metadata) {
            EntryKind::Link
        } else if metadata.is_dir() {
            EntryKind::Directory
        } else if metadata.is_file() {
            EntryKind::File
        } else {
            bail!("execution input path contains unsupported file");
        };
        let link = if kind == EntryKind::Link {
            Some(fs::read_link(&candidate)?)
        } else {
            None
        };
        let path_text = candidate
            .to_str()
            .context("input resolution path must be UTF-8")?;
        let link_text = link
            .as_ref()
            .map(|target| target.to_str().context("input link must be UTF-8"))
            .transpose()?;
        if path_text.len() > 4096 || link_text.is_some_and(|target| target.len() > 4096) {
            return Err(limit("input resolution path budget exhausted"));
        }
        used = used
            .checked_add(
                path_text.len() as u64 * 6
                    + link_text.map_or(0, |text| text.len() as u64 * 6)
                    + 256,
            )
            .context("resolution budget overflow")?;
        if used > maximum {
            return Err(limit("input resolution metadata budget exhausted"));
        }
        evidence.push(ResolutionEvidence {
            path: candidate.clone(),
            mode: mode(&candidate, &metadata)?,
            kind,
            link: link.clone(),
        });
        if let Some(target) = link {
            links += 1;
            if links > 40 {
                bail!("execution input has too many symbolic links");
            }
            if target.is_absolute() {
                current = path_root(&target);
            }
            let parts: Vec<_> = target
                .components()
                .filter(|part| !matches!(part, Component::RootDir | Component::Prefix(_)))
                .map(|part| part.as_os_str().to_owned())
                .collect();
            for part in parts.into_iter().rev() {
                pending.push_front(part);
            }
        } else {
            current = candidate;
        }
    }
    #[cfg(windows)]
    let current = current.canonicalize()?;
    Ok((current, evidence))
}
fn related(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}
pub(crate) fn absolute(config_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        config_dir.join(path)
    }
}
pub(crate) fn assert_outside(path: &Path, roots: &[(PathBuf, String)]) -> Result<()> {
    #[cfg(windows)]
    let resolved = crate::quality::load::prospective_path(path)?;
    #[cfg(windows)]
    let path = resolved.as_path();
    if roots.iter().any(|(root, _)| related(path, root)) {
        bail!("workspace/state path overlaps input root");
    }
    Ok(())
}
impl Inputs {
    pub fn inventory(
        root: &Path,
        config: &ExecutionConfig,
        config_dir: &Path,
        guard: &impl Fn() -> Result<()>,
    ) -> Result<Self> {
        Self::inventory_with_options(root, config, config_dir, guard, false)
    }
    pub fn inventory_external(
        root: &Path,
        config: &ExecutionConfig,
        config_dir: &Path,
        guard: &impl Fn() -> Result<()>,
    ) -> Result<Self> {
        Self::inventory_with_options(root, config, config_dir, guard, true)
    }
    fn inventory_with_options(
        root: &Path,
        config: &ExecutionConfig,
        config_dir: &Path,
        guard: &impl Fn() -> Result<()>,
        include_git: bool,
    ) -> Result<Self> {
        let requested_root = root.to_owned();
        let (root, mut resolutions) =
            resolve_evidence(root, config.limits.max_inventory_bytes, guard)?;
        if !root.is_dir() {
            bail!("mutation input root must be a directory");
        }
        let mut roots = vec![(root.clone(), String::new())];
        for dependency in &config.workspace.dependencies {
            let (source, evidence) = resolve_evidence(
                &absolute(config_dir, &dependency.source),
                config.limits.max_inventory_bytes,
                guard,
            )
            .context("declared dependency source unavailable")?;
            resolutions.extend(evidence);
            storage::encoded_size(&resolutions, config.limits.max_inventory_bytes)?;
            if roots.iter().any(|(existing, _)| related(&source, existing)) {
                bail!("dependency source roots overlap");
            }
            if roots.iter().skip(1).any(|(_, destination)| {
                related(Path::new(destination), Path::new(&dependency.destination))
            }) {
                bail!("dependency destinations overlap");
            }
            roots.push((source, dependency.destination.clone()));
        }
        let include = Matcher::new(&config.workspace.include)?;
        let exclude = Matcher::new(&config.workspace.exclude)?;
        let mut entries = Vec::new();
        let mut bytes = 0u64;
        let mut metadata_bytes =
            storage::encoded_size(&resolutions, config.limits.max_inventory_bytes)?;
        let mut visited = 0usize;
        for (source, destination) in &roots {
            let mut pending = vec![(source.clone(), destination.clone())];
            while let Some((path, relative)) = pending.pop() {
                guard()?;
                visited = visited.checked_add(1).context("inventory count overflow")?;
                if visited > config.limits.max_workspace_files {
                    return Err(limit("workspace discovery entry budget exhausted"));
                }
                if !relative.is_empty() {
                    validate_relative_path(&relative)?;
                }
                if !include_git && relative.split('/').any(|part| part == ".git") {
                    continue;
                }
                let original = destination.is_empty();
                if original && excluded(&relative, &exclude) {
                    continue;
                }
                let metadata = fs::symlink_metadata(&path)?;
                let kind = if is_link(&metadata) {
                    EntryKind::Link
                } else if metadata.is_dir() {
                    EntryKind::Directory
                } else if metadata.is_file() {
                    EntryKind::File
                } else {
                    bail!("unsupported special workspace input");
                };
                // Traversal remains bounded even when selection excludes most files.
                if kind == EntryKind::Directory {
                    for child in fs::read_dir(&path)? {
                        guard()?;
                        let child = child?;
                        let name = child
                            .file_name()
                            .into_string()
                            .map_err(|_| anyhow::anyhow!("workspace names must be UTF-8"))?;
                        let child_relative = if relative.is_empty() {
                            name
                        } else {
                            format!("{relative}/{name}")
                        };
                        if pending.len().saturating_add(visited)
                            >= config.limits.max_workspace_files
                        {
                            return Err(limit("workspace discovery entry budget exhausted"));
                        }
                        if metadata_bytes
                            .checked_add(child_relative.len() as u64 * 6 + 256)
                            .is_none_or(|v| v > config.limits.max_inventory_bytes)
                        {
                            return Err(limit("workspace metadata byte budget exhausted"));
                        }
                        // Reserve pending path memory before allocating the next queue entry.
                        metadata_bytes += child_relative.len() as u64 * 6 + 256;
                        pending.push((child.path(), child_relative));
                    }
                }
                if relative.is_empty() {
                    continue;
                }
                if original && !include.matches(&relative) {
                    continue;
                }
                let mode = mode(&path, &metadata)?;
                let (length, digest, link) = match kind {
                    EntryKind::File => {
                        if metadata.len() > config.limits.max_workspace_file_bytes {
                            return Err(limit("workspace input file byte budget exhausted"));
                        }
                        bytes = bytes
                            .checked_add(metadata.len())
                            .context("workspace byte count overflow")?;
                        if bytes > config.limits.max_workspace_bytes {
                            return Err(limit("workspace input byte budget exhausted"));
                        }
                        (
                            metadata.len(),
                            hash_file(&path, config.limits.max_workspace_file_bytes, guard)?,
                            None,
                        )
                    }
                    EntryKind::Directory => (0, String::new(), None),
                    EntryKind::Link => {
                        let written = fs::read_link(&path)?;
                        let target = path
                            .canonicalize()
                            .context("dangling or circular workspace link")?;
                        let target_destination = roots
                            .iter()
                            .filter_map(|(base, dest)| {
                                target
                                    .strip_prefix(base)
                                    .ok()
                                    .map(|suffix| (base.components().count(), dest, suffix))
                            })
                            .max_by_key(|(length, _, _)| *length)
                            .context("workspace link leaves declared copied roots")?;
                        let target_path =
                            Path::new(target_destination.1).join(target_destination.2);
                        let target_text = portable_relative(&target_path)?;
                        if !target_text.is_empty() {
                            validate_relative_path(&target_text)?;
                        }
                        let relocated = relative_link(
                            Path::new(&relative).parent().unwrap_or(Path::new("")),
                            &target_path,
                        )?;
                        // Both original and relocated targets participate in identity.
                        let written = written.to_str().context("link text must be UTF-8")?;
                        if written.len() > 4096 {
                            return Err(limit("workspace link byte budget exhausted"));
                        }
                        (0, storage::digest(written.as_bytes()), Some(relocated))
                    }
                };
                let entry_cost = relative.len() as u64 * 6
                    + link.as_ref().map_or(0, |v| v.len() as u64 * 6)
                    + 512;
                metadata_bytes = metadata_bytes
                    .checked_add(entry_cost)
                    .context("metadata byte count overflow")?;
                if metadata_bytes > config.limits.max_inventory_bytes {
                    return Err(limit("workspace metadata byte budget exhausted"));
                }
                if entries.len() >= config.limits.max_workspace_files {
                    return Err(limit("workspace entry budget exhausted"));
                }
                entries.push(Entry {
                    path: relative,
                    kind,
                    bytes: length,
                    mode,
                    digest,
                    link,
                    source: path,
                });
            }
        }
        // Preserve only selected directory records and the parents required by selected leaves.
        let mut present: BTreeSet<String> =
            entries.iter().map(|entry| entry.path.clone()).collect();
        let mut parents = Vec::new();
        for entry in &entries {
            let mut parent = Path::new(&entry.path).parent();
            while let Some(path) = parent {
                if path.as_os_str().is_empty() {
                    break;
                }
                let text = portable_relative(path)?;
                if !present.contains(&text) {
                    metadata_bytes = metadata_bytes
                        .checked_add(text.len() as u64 * 6 + 512)
                        .context("parent metadata overflow")?;
                    if metadata_bytes > config.limits.max_inventory_bytes
                        || present.len() >= config.limits.max_workspace_files
                    {
                        return Err(limit("selected parent metadata budget exhausted"));
                    }
                    present.insert(text.clone());
                    let source = root.join(path);
                    let (source, mode) = match fs::symlink_metadata(&source) {
                        Ok(metadata) if metadata.is_dir() => {
                            let mode = mode(&source, &metadata)?;
                            (source, mode)
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                            (PathBuf::new(), if cfg!(windows) { 0o666 } else { 0o755 })
                        }
                        _ => bail!("selected parent is not a copied directory"),
                    };
                    parents.push(Entry {
                        path: text,
                        kind: EntryKind::Directory,
                        bytes: 0,
                        mode,
                        digest: String::new(),
                        link: None,
                        source,
                    });
                }
                parent = path.parent();
            }
        }
        entries.extend(parents);
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        let mut paths = BTreeSet::new();
        for entry in &entries {
            if !paths.insert(entry.path.as_str()) {
                bail!("dependency destination conflicts with copied input");
            }
        }
        for entry in &entries {
            if let Some(link) = &entry.link {
                let target = normalize_relative(
                    Path::new(&entry.path)
                        .parent()
                        .unwrap_or(Path::new(""))
                        .join(link),
                )?;
                if !target.as_os_str().is_empty()
                    && !paths.contains(portable_relative(&target)?.as_str())
                {
                    bail!("workspace link target is excluded from copied inventory");
                }
            }
        }
        let root_mode = mode(&root, &fs::metadata(&root)?)?;
        storage::encoded_size(
            &(&entries, &resolutions, root_mode),
            config.limits.max_inventory_bytes,
        )?;
        Ok(Self {
            requested_root,
            root,
            roots,
            entries,
            bytes,
            root_mode,
            resolutions,
        })
    }
    pub fn verify(
        &self,
        config: &ExecutionConfig,
        config_dir: &Path,
        guard: &impl Fn() -> Result<()>,
    ) -> Result<()> {
        let current = Self::inventory(&self.requested_root, config, config_dir, guard)?;
        if self.entries != current.entries
            || self.roots != current.roots
            || self.root_mode != current.root_mode
            || self.resolutions != current.resolutions
        {
            bail!("execution inputs changed during the run");
        }
        Ok(())
    }
    pub fn copy_to(&self, destination: &Path, guard: &impl Fn() -> Result<()>) -> Result<()> {
        fs::create_dir(destination)?;
        for entry in &self.entries {
            guard()?;
            let target = destination.join(&entry.path);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            match entry.kind {
                EntryKind::Directory => fs::create_dir_all(&target)?,
                EntryKind::File => {
                    let mut source = open_regular(&entry.source)?;
                    let metadata = source.metadata()?;
                    if metadata.len() != entry.bytes {
                        bail!("copied input length changed");
                    }
                    let mut options = OpenOptions::new();
                    options.write(true).create_new(true);
                    #[cfg(unix)]
                    options.mode(0o600);
                    let mut output = options.open(&target)?;
                    let mut digest = Sha256::new();
                    let mut total = 0u64;
                    let mut buffer = [0u8; 65536];
                    loop {
                        guard()?;
                        let count = source.read(&mut buffer)?;
                        if count == 0 {
                            break;
                        }
                        total = total
                            .checked_add(count as u64)
                            .context("copy byte overflow")?;
                        if total > entry.bytes {
                            bail!("copied input grew");
                        }
                        digest.update(&buffer[..count]);
                        output.write_all(&buffer[..count])?;
                    }
                    if total != entry.bytes
                        || hex_digest(digest.finalize().as_slice()) != entry.digest
                    {
                        bail!("copied input bytes changed");
                    }
                    set_mode(&target, entry.mode)?;
                }
                EntryKind::Link => copy_link(
                    entry.link.as_ref().context("missing link target")?,
                    &target,
                    &entry.source,
                )?,
            }
        }
        // Apply directory modes after children have been created, so read-only trees copy correctly.
        for entry in self
            .entries
            .iter()
            .rev()
            .filter(|entry| entry.kind == EntryKind::Directory)
        {
            set_mode(&destination.join(&entry.path), entry.mode)?;
        }
        set_mode(destination, self.root_mode)?;
        Ok(())
    }
}
fn excluded(path: &str, matcher: &Matcher) -> bool {
    let mut prefix = String::new();
    for component in path.split('/') {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(component);
        if matcher.matches(&prefix) || matcher.matches(&format!("{prefix}/")) {
            return true;
        }
    }
    false
}
#[cfg(unix)]
fn open_regular(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK | nix::libc::O_CLOEXEC)
        .open(path)?;
    if !file.metadata()?.is_file() {
        bail!("execution input must be regular");
    }
    Ok(file)
}
pub(crate) fn hash_file(
    path: &Path,
    maximum: u64,
    guard: &impl Fn() -> Result<()>,
) -> Result<String> {
    let mut file = open_regular(path)?;
    let before = file.metadata()?;
    if before.len() > maximum {
        return Err(limit("input file byte budget exhausted"));
    }
    let mut digest = Sha256::new();
    let mut length = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        guard()?;
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        length = length
            .checked_add(count as u64)
            .context("hash length overflow")?;
        if length > maximum {
            return Err(limit("input file grew beyond byte budget"));
        }
        digest.update(&buffer[..count]);
    }
    let after = file.metadata()?;
    if length != before.len() || !same_snapshot(&before, &after) {
        bail!("input changed while hashing");
    }
    Ok(hex_digest(digest.finalize().as_slice()))
}
fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn normalize_relative(path: PathBuf) -> Result<PathBuf> {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => result.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() {
                    bail!("link escapes workspace");
                }
            }
            _ => bail!("absolute relocated workspace link"),
        }
    }
    Ok(result)
}
fn relative_link(parent: &Path, target: &Path) -> Result<String> {
    let from: Vec<_> = parent.components().collect();
    let to: Vec<_> = target.components().collect();
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut result = PathBuf::new();
    for _ in common..from.len() {
        result.push("..");
    }
    for part in &to[common..] {
        result.push(part.as_os_str());
    }
    if result.as_os_str().is_empty() {
        result.push(".");
    }
    portable_relative(&result)
}

#[cfg(unix)]
type DirectoryIdentity = (u64, u64);
#[cfg(windows)]
type DirectoryIdentity = (u64, [u8; 16]);
pub(crate) struct OwnedDirectory {
    pub path: PathBuf,
    pub cleanup_allowed: bool,
    identity: DirectoryIdentity,
}
impl OwnedDirectory {
    pub fn create(parent: &Path, roots: &[(PathBuf, String)]) -> Result<Self> {
        let parent = parent.canonicalize()?;
        if roots.iter().any(|(root, _)| parent.starts_with(root)) {
            bail!("workspace parent is inside an input root");
        }
        let path = storage::create_private_directory(&parent, "archguard-mutator")?;
        assert_outside(&path, roots)?;
        #[cfg(windows)]
        if let Err(error) = crate::windows_fs::check_cleanup_capability(&path) {
            let _ = fs::remove_dir(&path);
            return Err(error);
        }
        Self::from_path(path)
    }
    pub fn from_path(path: PathBuf) -> Result<Self> {
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_dir() {
            bail!("owned workspace is not a directory");
        }
        let identity = directory_identity(&path)?;
        Ok(Self {
            path,
            cleanup_allowed: true,
            identity,
        })
    }
    pub fn cleanup(&mut self) -> Result<()> {
        if !self.cleanup_allowed {
            bail!("workspace preserved because process cleanup is uncertain");
        }
        remove_owned(&self.path, self.identity)
    }
}
impl Drop for OwnedDirectory {
    fn drop(&mut self) {
        if self.cleanup_allowed {
            let _ = remove_owned(&self.path, self.identity);
        }
    }
}
struct DirectoryQueueBudget {
    entries: usize,
    queued_bytes: u64,
    maximum_entries: usize,
    maximum_bytes: u64,
}
impl DirectoryQueueBudget {
    fn entry(&mut self, parent: &Path, name: &std::ffi::OsStr) -> Result<()> {
        self.entries = self
            .entries
            .checked_add(1)
            .context("directory entry count overflow")?;
        if self.entries > self.maximum_entries {
            return Err(limit("directory traversal entry budget exhausted"));
        }
        if parent
            .as_os_str()
            .len()
            .saturating_add(name.len())
            .saturating_add(1)
            > 4096
        {
            return Err(limit("generated directory path budget exhausted"));
        }
        Ok(())
    }
    fn push_length(&mut self, length: usize) -> Result<()> {
        let bytes = length as u64 * 2 + 128;
        self.queued_bytes = self
            .queued_bytes
            .checked_add(bytes)
            .context("directory queue byte overflow")?;
        if self.queued_bytes > self.maximum_bytes {
            return Err(limit("directory queue metadata budget exhausted"));
        }
        Ok(())
    }
    fn push(&mut self, path: &Path) -> Result<()> {
        self.push_length(path.as_os_str().len())
    }
    fn pop(&mut self, path: &Path) {
        self.queued_bytes = self
            .queued_bytes
            .saturating_sub(path.as_os_str().len() as u64 * 2 + 128);
    }
}
fn remove_owned(path: &Path, identity: DirectoryIdentity) -> Result<()> {
    remove_owned_until(path, identity, Instant::now() + Duration::from_secs(30))
}
#[cfg(unix)]
fn remove_owned_until(path: &Path, identity: DirectoryIdentity, deadline: Instant) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_dir() || (metadata.dev(), metadata.ino()) != identity {
        bail!("owned workspace identity changed; refusing cleanup");
    } // Make owned read-only directories removable without following links.
    let mut budget = DirectoryQueueBudget {
        entries: 0,
        queued_bytes: 0,
        maximum_entries: 34000000,
        maximum_bytes: 134217728,
    };
    let parent_path = path.parent().context("owned workspace has no parent")?;
    let name = path.file_name().context("owned workspace has no name")?;
    let parent = OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_DIRECTORY | nix::libc::O_CLOEXEC)
        .open(parent_path)?;
    let root = File::from(nix::fcntl::openat(
        &parent,
        name,
        nix::fcntl::OFlag::O_RDONLY
            | nix::fcntl::OFlag::O_DIRECTORY
            | nix::fcntl::OFlag::O_NOFOLLOW
            | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    )?);
    let metadata = root.metadata()?;
    if (metadata.dev(), metadata.ino()) != identity {
        bail!("owned workspace identity changed before cleanup open");
    }
    budget.push(path)?;
    let mut pending = vec![(path.to_owned(), false, identity)];
    while let Some((directory, remove, expected)) = pending.pop() {
        budget.pop(&directory);
        if Instant::now() >= deadline {
            return Err(limit("owned workspace cleanup deadline reached"));
        }
        let relative = directory.strip_prefix(path)?;
        if remove {
            let (containing, name) = if relative.as_os_str().is_empty() {
                (parent.try_clone()?, name)
            } else {
                (
                    open_owned_directory(&root, relative.parent().unwrap_or(Path::new("")))?,
                    relative
                        .file_name()
                        .context("owned directory has no name")?,
                )
            };
            let metadata = nix::sys::stat::fstatat(
                &containing,
                name,
                nix::fcntl::AtFlags::AT_SYMLINK_NOFOLLOW,
            )?;
            if (metadata.st_dev as u64, metadata.st_ino as u64) != expected {
                bail!("owned directory identity changed before deletion");
            }
            nix::unistd::unlinkat(&containing, name, nix::unistd::UnlinkatFlags::RemoveDir)?;
            continue;
        }
        let opened = open_owned_directory(&root, relative)?;
        let metadata = opened.metadata()?;
        if (metadata.dev(), metadata.ino()) != expected {
            bail!("owned directory identity changed before traversal");
        }
        opened.set_permissions(fs::Permissions::from_mode(0o700))?;
        budget.push(&directory)?;
        pending.push((directory.clone(), true, expected));
        visit_directory(&opened, |name| {
            if Instant::now() >= deadline {
                return Err(limit("owned workspace cleanup deadline reached"));
            }
            budget.entry(&directory, name)?;
            let metadata =
                nix::sys::stat::fstatat(&opened, name, nix::fcntl::AtFlags::AT_SYMLINK_NOFOLLOW)?;
            if metadata.st_mode & nix::libc::S_IFMT == nix::libc::S_IFDIR {
                budget.push_length(directory.as_os_str().len() + 1 + name.len())?;
                pending.push((
                    directory.join(name),
                    false,
                    (metadata.st_dev as u64, metadata.st_ino as u64),
                ));
            } else {
                nix::unistd::unlinkat(&opened, name, nix::unistd::UnlinkatFlags::NoRemoveDir)?;
            }
            Ok(())
        })?;
    }
    Ok(())
}
#[cfg(unix)]
fn open_owned_directory(root: &File, relative: &Path) -> Result<File> {
    let mut opened = root.try_clone()?;
    for component in relative.components() {
        let Component::Normal(name) = component else {
            bail!("unsafe owned directory traversal");
        };
        opened = File::from(nix::fcntl::openat(
            &opened,
            name,
            nix::fcntl::OFlag::O_RDONLY
                | nix::fcntl::OFlag::O_DIRECTORY
                | nix::fcntl::OFlag::O_NOFOLLOW
                | nix::fcntl::OFlag::O_CLOEXEC,
            nix::sys::stat::Mode::empty(),
        )?);
    }
    Ok(opened)
}
#[cfg(unix)]
fn visit_directory(
    directory: &File,
    mut visitor: impl FnMut(&std::ffi::OsStr) -> Result<()>,
) -> Result<()> {
    struct Directory(*mut nix::libc::DIR);
    impl Drop for Directory {
        fn drop(&mut self) {
            unsafe {
                nix::libc::closedir(self.0);
            }
        }
    }
    let descriptor = directory.try_clone()?.into_raw_fd();
    // fdopendir takes ownership only on success; the guard closes it afterward.
    let pointer = unsafe { nix::libc::fdopendir(descriptor) };
    if pointer.is_null() {
        let error = std::io::Error::last_os_error();
        unsafe {
            nix::libc::close(descriptor);
        }
        return Err(error.into());
    }
    let directory = Directory(pointer);
    loop {
        nix::errno::Errno::clear();
        let entry = unsafe { nix::libc::readdir(directory.0) };
        if entry.is_null() {
            let error = nix::errno::Errno::last_raw();
            if error != 0 {
                return Err(std::io::Error::from_raw_os_error(error).into());
            }
            return Ok(());
        }
        // readdir names remain valid until the next call on this owned DIR.
        let name = unsafe { std::ffi::CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
        if name == b"." || name == b".." {
            continue;
        }
        visitor(std::ffi::OsStr::from_bytes(name))?;
    }
}
pub(crate) fn disk_usage(
    root: &Path,
    limits: &ExecutionLimits,
    guard: &impl Fn() -> Result<()>,
) -> Result<u64> {
    let mut budget = DirectoryQueueBudget {
        entries: 0,
        queued_bytes: 0,
        maximum_entries: limits
            .max_workspace_files
            .saturating_mul(limits.workers + 2),
        maximum_bytes: limits.max_inventory_bytes,
    };
    budget.push(root)?;
    let mut pending = vec![root.to_owned()];
    let mut bytes = 0u64;
    while let Some(directory) = pending.pop() {
        budget.pop(&directory);
        guard()?;
        let metadata = match fs::symlink_metadata(&directory) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        if !metadata.is_dir() {
            continue;
        }
        let children = match fs::read_dir(&directory) {
            Ok(children) => children,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        for child in children {
            guard()?;
            let child = child?;
            budget.entry(&directory, &child.file_name())?;
            let metadata = match fs::symlink_metadata(child.path()) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            if metadata.is_dir() {
                budget.push_length(directory.as_os_str().len() + 1 + child.file_name().len())?;
                pending.push(child.path());
            } else if metadata.is_file() {
                bytes = bytes
                    .checked_add(metadata.len())
                    .context("generated byte overflow")?;
                if bytes > limits.max_total_workspace_bytes {
                    return Err(limit("aggregate workspace disk budget exhausted"));
                }
            } else if !is_link(&metadata) {
                bail!("command created unsupported workspace file");
            }
        }
    }
    Ok(bytes)
}

fn portable_relative(path: &Path) -> Result<String> {
    let text = path.to_str().context("relative path must be UTF-8")?;
    #[cfg(windows)]
    {
        Ok(text.replace('\\', "/"))
    }
    #[cfg(unix)]
    {
        Ok(text.to_owned())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use crate::mutator::workspace::*;
    fn directory() -> PathBuf {
        storage::create_private_directory(&std::env::temp_dir(), "archguard-workspace-test")
            .unwrap()
    }
    fn config() -> ExecutionConfig {
        serde_json::from_str(r#"{"command":["node"]}"#).unwrap()
    }
    #[test]
    fn unchanged_input_alias_verifies_and_retargeted_ancestor_is_rejected() {
        let root = directory();
        for name in ["first", "second"] {
            fs::create_dir(root.join(name)).unwrap();
            fs::create_dir(root.join(name).join("input")).unwrap();
            fs::write(
                root.join(name).join("input/source.ts"),
                "export const value = true;",
            )
            .unwrap();
        }
        symlink("first", root.join("ancestor")).unwrap();
        let requested = root.join("ancestor/input");
        let config = config();
        let inputs = Inputs::inventory(&requested, &config, &root, &|| Ok(())).unwrap();
        inputs.verify(&config, &root, &|| Ok(())).unwrap();
        fs::remove_file(root.join("ancestor")).unwrap();
        symlink("second", root.join("ancestor")).unwrap();
        assert!(inputs.verify(&config, &root, &|| Ok(())).is_err());
        fs::remove_file(root.join("ancestor")).unwrap();
        symlink("first", root.join("ancestor")).unwrap();
        inputs.verify(&config, &root, &|| Ok(())).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn isolated_copies_relocate_links_and_revalidate_membership_bytes_modes() {
        let root = directory();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src/a.ts"), "export const n = 1;\n").unwrap();
        symlink("src/a.ts", root.join("alias.ts")).unwrap();
        let config = config();
        let inputs = Inputs::inventory(&root, &config, &root, &|| Ok(())).unwrap();
        let mut owned = OwnedDirectory::create(&std::env::temp_dir(), &inputs.roots).unwrap();
        let copy = owned.path.join("copy");
        inputs.copy_to(&copy, &|| Ok(())).unwrap();
        fs::write(copy.join("src/a.ts"), "changed only in copy").unwrap();
        assert_eq!(
            fs::read_to_string(root.join("src/a.ts")).unwrap(),
            "export const n = 1;\n"
        );
        assert_eq!(
            fs::read_to_string(copy.join("alias.ts")).unwrap(),
            "changed only in copy"
        );
        inputs.verify(&config, &root, &|| Ok(())).unwrap();
        fs::write(root.join("helper.txt"), "added").unwrap();
        assert!(inputs.verify(&config, &root, &|| Ok(())).is_err());
        fs::remove_file(root.join("helper.txt")).unwrap();
        inputs.verify(&config, &root, &|| Ok(())).unwrap();
        fs::set_permissions(root.join("src/a.ts"), fs::Permissions::from_mode(0o700)).unwrap();
        assert!(inputs.verify(&config, &root, &|| Ok(())).is_err());
        fs::set_permissions(root.join("src/a.ts"), fs::Permissions::from_mode(0o644)).unwrap();
        fs::write(root.join("src/a.ts"), "stale").unwrap();
        assert!(
            inputs
                .copy_to(&owned.path.join("stale"), &|| Ok(()))
                .is_err()
        );
        let fresh = Inputs::inventory(&root, &config, &root, &|| Ok(())).unwrap();
        fresh
            .copy_to(&owned.path.join("fresh"), &|| Ok(()))
            .unwrap();
        owned.cleanup().unwrap();
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn budgets_external_links_and_excluded_targets_fail_then_correct() {
        let root = directory();
        let outside = directory();
        fs::write(root.join("a.ts"), "const a = 1;").unwrap();
        fs::write(outside.join("dep.txt"), "dep").unwrap();
        symlink(outside.join("dep.txt"), root.join("dep-link")).unwrap();
        let mut config = config();
        assert!(Inputs::inventory(&root, &config, &root, &|| Ok(())).is_err());
        config
            .workspace
            .dependencies
            .push(crate::mutator::config::DependencyCopy {
                source: outside.clone(),
                destination: "deps".into(),
            });
        let inputs = Inputs::inventory(&root, &config, &root, &|| Ok(())).unwrap();
        let mut owned = OwnedDirectory::create(&std::env::temp_dir(), &inputs.roots).unwrap();
        inputs
            .copy_to(&owned.path.join("copy"), &|| Ok(()))
            .unwrap();
        assert_eq!(fs::read(owned.path.join("copy/dep-link")).unwrap(), b"dep");
        owned.cleanup().unwrap();
        config.limits.max_inventory_bytes = 128;
        let error = Inputs::inventory(&root, &config, &root, &|| Ok(()))
            .err()
            .unwrap();
        assert!(error.downcast_ref::<WorkspaceLimit>().is_some());
        config.limits.max_inventory_bytes = 16777216;
        Inputs::inventory(&root, &config, &root, &|| Ok(())).unwrap();
        config.limits.max_workspace_file_bytes = 1;
        assert!(Inputs::inventory(&root, &config, &root, &|| Ok(())).is_err());
        config.limits.max_workspace_file_bytes = 268435456;
        config.workspace.dependencies.clear();
        fs::remove_file(root.join("dep-link")).unwrap();
        symlink("a.ts", root.join("alias")).unwrap();
        config.workspace.exclude.push("a.ts".into());
        assert!(Inputs::inventory(&root, &config, &root, &|| Ok(())).is_err());
        config.workspace.exclude.pop();
        Inputs::inventory(&root, &config, &root, &|| Ok(())).unwrap();
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
    #[test]
    fn cleanup_refuses_replaced_root_and_preserves_external_permissions() {
        let outside = directory();
        let mut owned = OwnedDirectory::create(&std::env::temp_dir(), &[]).unwrap();
        let path = owned.path.clone();
        fs::remove_dir(&path).unwrap();
        symlink(&outside, &path).unwrap();
        let before = fs::metadata(&outside).unwrap().mode();
        assert!(owned.cleanup().is_err());
        owned.cleanup_allowed = false;
        assert_eq!(fs::metadata(&outside).unwrap().mode(), before);
        fs::remove_file(path).unwrap();
        fs::remove_dir(outside).unwrap();
    }
}

#[cfg(all(test, unix))]
mod bounded_queue_tests {
    use crate::mutator::workspace::*;
    #[test]
    fn generated_queue_is_bounded_before_push_then_recovers() {
        let root = storage::create_private_directory(&std::env::temp_dir(), "archguard-queue-test")
            .unwrap();
        for index in 0..40 {
            fs::create_dir(root.join(format!("directory-{index}"))).unwrap();
        }
        let mut limits = ExecutionLimits {
            max_inventory_bytes: 1024,
            ..Default::default()
        };
        let error = disk_usage(&root, &limits, &|| Ok(())).unwrap_err();
        assert!(error.downcast_ref::<WorkspaceLimit>().is_some());
        limits.max_inventory_bytes = 16777216;
        assert_eq!(disk_usage(&root, &limits, &|| Ok(())).unwrap(), 0);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn cleanup_deadline_preserves_owned_tree_then_bounded_retry_removes_it() {
        let root =
            storage::create_private_directory(&std::env::temp_dir(), "archguard-cleanup-test")
                .unwrap();
        fs::create_dir(root.join("nested")).unwrap();
        fs::write(root.join("nested/source.ts"), "export const value = true;").unwrap();
        fs::set_permissions(root.join("nested"), fs::Permissions::from_mode(0o500)).unwrap();
        let metadata = fs::symlink_metadata(&root).unwrap();
        let identity = (metadata.dev(), metadata.ino());
        let error = remove_owned_until(&root, identity, Instant::now()).unwrap_err();
        assert!(error.downcast_ref::<WorkspaceLimit>().is_some());
        assert!(root.join("nested/source.ts").is_file());
        remove_owned(&root, identity).unwrap();
        assert!(!root.exists());
    }
    #[test]
    fn cleanup_unlinks_child_directory_links_without_touching_external_files_or_modes() {
        let owned = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        fs::write(
            external.path().join("source.ts"),
            "export const value = true;",
        )
        .unwrap();
        fs::set_permissions(external.path(), fs::Permissions::from_mode(0o500)).unwrap();
        symlink(external.path(), owned.path().join("linked-directory")).unwrap();
        let metadata = fs::symlink_metadata(owned.path()).unwrap();
        remove_owned(owned.path(), (metadata.dev(), metadata.ino())).unwrap();
        assert!(!owned.path().exists());
        assert_eq!(
            fs::read_to_string(external.path().join("source.ts")).unwrap(),
            "export const value = true;"
        );
        assert_eq!(fs::metadata(external.path()).unwrap().mode() & 0o777, 0o500);
        fs::set_permissions(external.path(), fs::Permissions::from_mode(0o700)).unwrap();
    }
    #[test]
    fn declared_link_chain_root_modes_and_git_inputs_remain_in_identity() {
        let root =
            storage::create_private_directory(&std::env::temp_dir(), "archguard-resolution-test")
                .unwrap();
        let external = root.join("external");
        fs::create_dir(&external).unwrap();
        fs::create_dir(external.join(".git")).unwrap();
        fs::write(external.join(".git/config"), "one").unwrap();
        symlink("external", root.join("inner")).unwrap();
        symlink("inner", root.join("outer")).unwrap();
        let path = root.join("outer");
        let (target, first) = resolve_evidence(&path, 16777216, &|| Ok(())).unwrap();
        assert_eq!(target, external);
        fs::remove_file(root.join("inner")).unwrap();
        symlink("./external", root.join("inner")).unwrap();
        let (target, second) = resolve_evidence(&path, 16777216, &|| Ok(())).unwrap();
        assert_eq!(target, external);
        assert_ne!(first, second);
        let mut config: ExecutionConfig = serde_json::from_str(r#"{"command":["node"]}"#).unwrap();
        config.workspace.exclude.clear();
        let first = Inputs::inventory_external(&path, &config, &root, &|| Ok(())).unwrap();
        assert!(
            first
                .entries
                .iter()
                .any(|entry| entry.path == ".git/config")
        );
        fs::write(external.join(".git/config"), "two").unwrap();
        let second = Inputs::inventory_external(&path, &config, &root, &|| Ok(())).unwrap();
        assert_ne!(first.entries, second.entries);
        fs::set_permissions(&external, fs::Permissions::from_mode(0o700)).unwrap();
        let third = Inputs::inventory_external(&path, &config, &root, &|| Ok(())).unwrap();
        assert_ne!(second.root_mode, third.root_mode);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn only_selected_parents_are_copied_with_declared_destinations() {
        let root =
            storage::create_private_directory(&std::env::temp_dir(), "archguard-selected-test")
                .unwrap();
        let dependency =
            storage::create_private_directory(&std::env::temp_dir(), "archguard-dependency-test")
                .unwrap();
        fs::create_dir(root.join("node_modules")).unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src/a.ts"), "true").unwrap();
        fs::write(dependency.join("runtime.js"), "ok").unwrap();
        let mut config: ExecutionConfig = serde_json::from_str(r#"{"command":["node"]}"#).unwrap();
        config.workspace.include = vec!["src/a.ts".into()];
        config
            .workspace
            .dependencies
            .push(crate::mutator::config::DependencyCopy {
                source: dependency.clone(),
                destination: "node_modules".into(),
            });
        let inputs = Inputs::inventory(&root, &config, &root, &|| Ok(())).unwrap();
        assert_eq!(
            inputs
                .entries
                .iter()
                .filter(|entry| entry.path == "node_modules")
                .count(),
            1
        );
        assert!(inputs.entries.iter().any(|entry| entry.path == "src"));
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(dependency).unwrap();
    }
}

fn path_root(path: &Path) -> PathBuf {
    path.components()
        .take_while(|part| matches!(part, Component::Prefix(_) | Component::RootDir))
        .collect()
}
#[cfg(unix)]
fn mode(_path: &Path, metadata: &fs::Metadata) -> Result<u32> {
    Ok(metadata.mode() & 0o777)
}
#[cfg(windows)]
fn mode(path: &Path, metadata: &fs::Metadata) -> Result<u32> {
    crate::windows_fs::mode_for_path(path, metadata)
}
fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        metadata.file_type().is_symlink()
    }
    #[cfg(windows)]
    {
        crate::windows_fs::is_reparse(metadata)
    }
}
#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    Ok(())
}
#[cfg(windows)]
fn set_mode(path: &Path, mode: u32) -> Result<()> {
    crate::windows_fs::set_mode(path, mode)?;
    Ok(())
}
fn copy_link(link: &str, target: &Path, source: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        let _ = source;
        symlink(link, target)?;
    }
    #[cfg(windows)]
    {
        if fs::metadata(source)?.is_dir() {
            crate::windows_fs::create_junction(
                target,
                &target
                    .parent()
                    .context("directory link has no parent")?
                    .join(link),
            )?;
        } else {
            std::os::windows::fs::symlink_file(link, target)?;
        }
    }
    Ok(())
}
#[cfg(windows)]
fn open_regular(path: &Path) -> Result<File> {
    crate::windows_fs::open_regular(path)
}
fn same_snapshot(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        (
            left.dev(),
            left.ino(),
            left.len(),
            left.mtime(),
            left.mtime_nsec(),
            left.ctime(),
            left.ctime_nsec(),
        ) == (
            right.dev(),
            right.ino(),
            right.len(),
            right.mtime(),
            right.mtime_nsec(),
            right.ctime(),
            right.ctime_nsec(),
        )
    }
    #[cfg(windows)]
    {
        crate::windows_fs::same_snapshot(left, right)
    }
}
fn directory_identity(path: &Path) -> Result<DirectoryIdentity> {
    #[cfg(unix)]
    {
        let metadata = fs::symlink_metadata(path)?;
        Ok((metadata.dev(), metadata.ino()))
    }
    #[cfg(windows)]
    {
        let file = crate::windows_fs::open_owned(path, false)?;
        if crate::windows_fs::is_reparse(&file.metadata()?) {
            bail!("owned workspace cannot be a reparse point");
        }
        crate::windows_fs::identity(&file)
    }
}
#[cfg(windows)]
fn remove_owned_until(path: &Path, identity: DirectoryIdentity, deadline: Instant) -> Result<()> {
    let _ancestors =
        crate::windows_fs::hold_ancestors(path.parent().context("owned workspace has no parent")?)?;
    let root = match crate::windows_fs::open_owned(path, true) {
        Ok(file) => file,
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
        {
            return Ok(());
        }
        Err(error) => return Err(error),
    };
    if !root.metadata()?.is_dir()
        || crate::windows_fs::is_reparse(&root.metadata()?)
        || crate::windows_fs::identity(&root)? != identity
    {
        bail!("owned workspace identity changed; refusing cleanup");
    }
    let mut budget = DirectoryQueueBudget {
        entries: 0,
        queued_bytes: 0,
        maximum_entries: 34000000,
        maximum_bytes: 134217728,
    };
    fn visit(
        path: &Path,
        handle: &File,
        budget: &mut DirectoryQueueBudget,
        deadline: Instant,
        depth: usize,
    ) -> Result<()> {
        if depth > 256 {
            return Err(limit("owned workspace cleanup depth exhausted"));
        }
        budget.push(path)?;
        for entry in fs::read_dir(path)? {
            if Instant::now() >= deadline {
                return Err(limit("owned workspace cleanup deadline reached"));
            }
            let entry = entry?;
            budget.entry(path, &entry.file_name())?;
            let child = crate::windows_fs::open_owned(&entry.path(), true)?;
            let metadata = child.metadata()?;
            if metadata.is_dir() && !crate::windows_fs::is_reparse(&metadata) {
                visit(&entry.path(), &child, budget, deadline, depth + 1)?;
            } else {
                crate::windows_fs::remove_handle(&child)?;
            }
        }
        budget.pop(path);
        crate::windows_fs::remove_handle(handle)
    }
    visit(path, &root, &mut budget, deadline, 0)
}

#[cfg(test)]
#[cfg(windows)]
mod windows_tests {
    use crate::mutator::workspace::*;
    #[test]
    fn windows_absolute_roots_and_existing_ancestor_checks_preserve_inputs() {
        for (input, expected) in [
            (r"C:\repository\source", r"C:\"),
            (r"\\server\share\repository", r"\\server\share\"),
            (r"\\?\C:\repository", r"\\?\C:\"),
            (r"\\?\UNC\server\share\repository", r"\\?\UNC\server\share\"),
        ] {
            assert_eq!(path_root(Path::new(input)), Path::new(expected));
        }
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let plain = root
            .to_str()
            .unwrap()
            .strip_prefix(r"\\?\")
            .unwrap()
            .to_owned();
        let requested = Path::new(&plain).join("state-must-not-be-created");
        assert!(assert_outside(&requested, &[(root, String::new())]).is_err());
        assert!(!requested.exists());
    }
    #[test]
    #[allow(clippy::permissions_set_readonly_false)]
    fn readonly_directory_attribute_does_not_remove_source_write_access() {
        let root = tempfile::tempdir().unwrap();
        let input = root.path().join("input");
        fs::create_dir(&input).unwrap();
        let source = input.join("customized directory");
        fs::create_dir(&source).unwrap();
        let mut attributes = fs::metadata(&source).unwrap().permissions();
        attributes.set_readonly(true);
        fs::set_permissions(&source, attributes).unwrap();
        fs::write(source.join("child.ts"), b"export const enabled = true;").unwrap();
        let config: ExecutionConfig = serde_json::from_str(r#"{"command":["node"]}"#).unwrap();
        let inputs = Inputs::inventory(&input, &config, root.path(), &|| Ok(())).unwrap();
        let copy = root.path().join("copy");
        inputs.copy_to(&copy, &|| Ok(())).unwrap();
        let copied = copy.join("customized directory");
        assert!(fs::metadata(&copied).unwrap().permissions().readonly());
        fs::write(copied.join("another.ts"), b"export const enabled = false;").unwrap();
        assert!(!source.join("another.ts").exists());
        let mut owned = OwnedDirectory::from_path(copy).unwrap();
        owned.cleanup().unwrap();
        let mut attributes = fs::metadata(&source).unwrap().permissions();
        attributes.set_readonly(false);
        fs::set_permissions(source, attributes).unwrap();
    }
    #[test]
    fn synthetic_dependency_parents_allow_nested_directory_creation() {
        let root = tempfile::tempdir().unwrap();
        let input = root.path().join("input");
        let dependency = root.path().join("dependency");
        fs::create_dir(&input).unwrap();
        fs::create_dir(&dependency).unwrap();
        fs::write(input.join("subject.ts"), b"export const enabled = true;").unwrap();
        fs::write(
            dependency.join("index.ts"),
            b"export const dependency = true;",
        )
        .unwrap();
        let mut config: ExecutionConfig = serde_json::from_str(r#"{"command":["node"]}"#).unwrap();
        config
            .workspace
            .dependencies
            .push(crate::mutator::DependencyCopy {
                source: dependency,
                destination: "deps/sdk".into(),
            });
        let inputs = Inputs::inventory(&input, &config, root.path(), &|| Ok(())).unwrap();
        let copy = root.path().join("copy");
        inputs.copy_to(&copy, &|| Ok(())).unwrap();
        let parent = copy.join("deps");
        assert_eq!(
            mode(&parent, &fs::metadata(&parent).unwrap()).unwrap() & 0o222,
            0o222
        );
        fs::create_dir(parent.join("new-directory")).unwrap();
        fs::write(parent.join("new-file"), b"data").unwrap();
        let mut owned = OwnedDirectory::from_path(copy).unwrap();
        owned.cleanup().unwrap();
    }
    #[test]
    fn snapshot_remaps_junction_and_preserves_readonly_file() {
        let root = tempfile::tempdir().unwrap();
        let input = root.path().join("input with spaces");
        fs::create_dir(&input).unwrap();
        fs::create_dir(input.join("source")).unwrap();
        fs::write(input.join("source/a.ts"), b"export const enabled = true;").unwrap();
        crate::windows_fs::set_mode(&input.join("source/a.ts"), 0o444).unwrap();
        crate::windows_fs::create_junction(&input.join("alias"), &input.join("source")).unwrap();
        let config: ExecutionConfig = serde_json::from_str(r#"{"command":["node"]}"#).unwrap();
        let inputs = Inputs::inventory(&input, &config, root.path(), &|| Ok(())).unwrap();
        let copy = root.path().join("copy");
        inputs.copy_to(&copy, &|| Ok(())).unwrap();
        assert_eq!(
            fs::read(copy.join("alias/a.ts")).unwrap(),
            b"export const enabled = true;"
        );
        assert_eq!(
            copy.join("alias").canonicalize().unwrap(),
            copy.join("source").canonicalize().unwrap()
        );
        assert!(fs::write(copy.join("source/a.ts"), b"changed").is_err());
        crate::windows_fs::set_mode(&copy.join("source/a.ts"), 0o666).unwrap();
        fs::write(copy.join("source/a.ts"), b"changed").unwrap();
        assert_eq!(
            fs::read(input.join("source/a.ts")).unwrap(),
            b"export const enabled = true;"
        );
        let mut owned = OwnedDirectory::from_path(copy).unwrap();
        owned.cleanup().unwrap();
        fs::remove_dir(input.join("alias")).unwrap();
        crate::windows_fs::set_mode(&input.join("source/a.ts"), 0o666).unwrap();
    }
    #[test]
    fn cleanup_rejects_replacement_and_leaves_junction_target_untouched() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("owned");
        fs::create_dir(&path).unwrap();
        let mut owned = OwnedDirectory::from_path(path.clone()).unwrap();
        fs::rename(&path, root.path().join("original")).unwrap();
        fs::create_dir(&path).unwrap();
        fs::write(path.join("replacement"), b"preserved").unwrap();
        assert!(owned.cleanup().is_err());
        assert!(path.join("replacement").exists());
        owned.cleanup_allowed = false;
        fs::remove_dir_all(&path).unwrap();
        fs::rename(root.path().join("original"), &path).unwrap();
        let outside = root.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("keep"), b"preserved").unwrap();
        crate::windows_fs::create_junction(&path.join("linked"), &outside).unwrap();
        crate::windows_fs::set_mode(&path, 0o444).unwrap();
        let mut corrected = OwnedDirectory::from_path(path.clone()).unwrap();
        corrected.cleanup().unwrap();
        assert!(!path.exists());
        assert_eq!(fs::read(outside.join("keep")).unwrap(), b"preserved");
    }
}
