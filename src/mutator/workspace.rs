use crate::mutator::{
    config::{ExecutionConfig, ExecutionLimits},
    storage,
};
use crate::{config::Matcher, quality::validate_relative_path};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::os::fd::IntoRawFd;
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
    let mut current = PathBuf::from("/");
    let mut pending: VecDeque<_> = path
        .components()
        .filter(|part| !matches!(part, Component::RootDir))
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
        let kind = if metadata.file_type().is_symlink() {
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
            mode: metadata.mode() & 0o777,
            kind,
            link: link.clone(),
        });
        if let Some(target) = link {
            links += 1;
            if links > 40 {
                bail!("execution input has too many symbolic links");
            }
            if target.is_absolute() {
                current = PathBuf::from("/");
            }
            let parts: Vec<_> = target
                .components()
                .filter(|part| !matches!(part, Component::RootDir))
                .map(|part| part.as_os_str().to_owned())
                .collect();
            for part in parts.into_iter().rev() {
                pending.push_front(part);
            }
        } else {
            current = candidate;
        }
    }
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
                let kind = if metadata.file_type().is_symlink() {
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
                let mode = metadata.mode() & 0o777;
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
                        let target_text = target_path
                            .to_str()
                            .context("link target path must be UTF-8")?;
                        if !target_text.is_empty() {
                            validate_relative_path(target_text)?;
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
                let text = path.to_str().context("parent path encoding")?;
                if !present.contains(text) {
                    metadata_bytes = metadata_bytes
                        .checked_add(text.len() as u64 * 6 + 512)
                        .context("parent metadata overflow")?;
                    if metadata_bytes > config.limits.max_inventory_bytes
                        || present.len() >= config.limits.max_workspace_files
                    {
                        return Err(limit("selected parent metadata budget exhausted"));
                    }
                    present.insert(text.into());
                    let source = root.join(path);
                    let (source, mode) = match fs::symlink_metadata(&source) {
                        Ok(metadata) if metadata.is_dir() => (source, metadata.mode() & 0o777),
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                            (PathBuf::new(), 0o755)
                        }
                        _ => bail!("selected parent is not a copied directory"),
                    };
                    parents.push(Entry {
                        path: text.into(),
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
                    && !paths.contains(target.to_str().context("link target encoding")?)
                {
                    bail!("workspace link target is excluded from copied inventory");
                }
            }
        }
        let root_mode = fs::metadata(&root)?.mode() & 0o777;
        storage::encoded_size(
            &(&entries, &resolutions, root_mode),
            config.limits.max_inventory_bytes,
        )?;
        Ok(Self {
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
        let current = Self::inventory(&self.root, config, config_dir, guard)?;
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
                    let mut output = OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .mode(0o600)
                        .open(&target)?;
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
                    fs::set_permissions(&target, fs::Permissions::from_mode(entry.mode))?;
                }
                EntryKind::Link => {
                    symlink(entry.link.as_ref().context("missing link target")?, &target)?
                }
            }
        }
        // Apply directory modes after children have been created, so read-only trees copy correctly.
        for entry in self
            .entries
            .iter()
            .rev()
            .filter(|entry| entry.kind == EntryKind::Directory)
        {
            fs::set_permissions(
                destination.join(&entry.path),
                fs::Permissions::from_mode(entry.mode),
            )?;
        }
        fs::set_permissions(destination, fs::Permissions::from_mode(self.root_mode))?;
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
    if length != before.len()
        || (
            before.dev(),
            before.ino(),
            before.len(),
            before.mtime(),
            before.mtime_nsec(),
            before.ctime(),
            before.ctime_nsec(),
        ) != (
            after.dev(),
            after.ino(),
            after.len(),
            after.mtime(),
            after.mtime_nsec(),
            after.ctime(),
            after.ctime_nsec(),
        )
    {
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
    Ok(result.to_str().context("relocated link encoding")?.into())
}

pub(crate) struct OwnedDirectory {
    pub path: PathBuf,
    pub cleanup_allowed: bool,
    identity: (u64, u64),
}
impl OwnedDirectory {
    pub fn create(parent: &Path, roots: &[(PathBuf, String)]) -> Result<Self> {
        let parent = parent.canonicalize()?;
        if roots.iter().any(|(root, _)| parent.starts_with(root)) {
            bail!("workspace parent is inside an input root");
        }
        let path = storage::create_private_directory(&parent, "archguard-mutator")?;
        assert_outside(&path, roots)?;
        Self::from_path(path)
    }
    pub fn from_path(path: PathBuf) -> Result<Self> {
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_dir() {
            bail!("owned workspace is not a directory");
        }
        Ok(Self {
            path,
            cleanup_allowed: true,
            identity: (metadata.dev(), metadata.ino()),
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
fn remove_owned(path: &Path, identity: (u64, u64)) -> Result<()> {
    remove_owned_until(path, identity, Instant::now() + Duration::from_secs(30))
}
fn remove_owned_until(path: &Path, identity: (u64, u64), deadline: Instant) -> Result<()> {
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
            } else if !metadata.file_type().is_symlink() {
                bail!("command created unsupported workspace file");
            }
        }
    }
    Ok(bytes)
}

#[cfg(test)]
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

#[cfg(test)]
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
