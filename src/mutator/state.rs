use super::{
    config::{ReusePolicy, StateConfig},
    facts::MutationSite,
    result::{MutationOutcome, MutationResult, TestExecutionRequest, TestExecutionResult},
    storage,
    workspace::{self, OwnedDirectory},
};
use anyhow::{Context, Result, bail};
use nix::fcntl::{Flock, FlockArg};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

const MAX_RECORD_BYTES: u64 = 33554432;
const MAX_STATE_BYTES: u64 = 1073741824;
const MAX_ACTIVE_BYTES: u64 = 16384;
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    schema_version: u32,
    input_digest: String,
    mutation_id: String,
    cleanup_complete: bool,
    result: MutationResult,
    request: TestExecutionRequest,
    response: TestExecutionResult,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    schema_version: u32,
    checksum: String,
    record: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Active {
    schema_version: u32,
    run_id: String,
    workspace: PathBuf,
}

pub(crate) struct RunState {
    pub directory: PathBuf,
    pub reuse: ReusePolicy,
    _lock: Flock<File>,
    active: bool,
    bytes: std::cell::Cell<u64>,
    records: std::cell::Cell<usize>,
}
impl RunState {
    pub fn open(
        config: &StateConfig,
        config_dir: &Path,
        roots: &[(PathBuf, String)],
    ) -> Result<Self> {
        let requested = workspace::absolute(config_dir, &config.directory);
        if requested
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            bail!("state directory cannot contain parent traversal");
        }
        workspace::assert_outside(&requested, roots)?;
        // Never follow an existing state-directory link, including an ancestor link.
        let mut prefix = PathBuf::new();
        for part in requested.components() {
            prefix.push(part.as_os_str());
            match fs::symlink_metadata(&prefix) {
                Ok(metadata) => {
                    if metadata.file_type().is_symlink() || !metadata.is_dir() {
                        bail!("state directory contains a link or non-directory");
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    fs::create_dir(&prefix)?;
                    fs::set_permissions(&prefix, fs::Permissions::from_mode(0o700))?;
                }
                Err(error) => return Err(error.into()),
            }
        }
        let directory = requested.canonicalize()?;
        workspace::assert_outside(&directory, roots)?;
        let metadata = fs::metadata(&directory)?;
        if metadata.uid() != unsafe { nix::libc::geteuid() } || metadata.mode() & 0o077 != 0 {
            bail!("state directory must be owned by the current user with private permissions");
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK | nix::libc::O_CLOEXEC)
            .open(directory.join("lock"))?;
        let metadata = file.metadata()?;
        if !metadata.is_file()
            || metadata.uid() != unsafe { nix::libc::geteuid() }
            || metadata.mode() & 0o077 != 0
        {
            bail!("state lock must be a private regular file");
        }
        let lock = Flock::lock(file, FlockArg::LockExclusiveNonblock)
            .map_err(|(_, error)| anyhow::anyhow!("mutation state is locked: {error}"))?;
        if fs::symlink_metadata(directory.join("active.json")).is_ok() {
            let active: Active = serde_json::from_slice(&storage::read_regular(
                &directory.join("active.json"),
                MAX_ACTIVE_BYTES,
            )?)
            .context("interrupted state has invalid active-run metadata")?;
            if active.schema_version != 1
                || active.run_id.len() > 4096
                || !active.workspace.is_absolute()
                || active.workspace.as_os_str().len() > 4096
            {
                bail!("prior active-run metadata invalid");
            }
            bail!(
                "prior run {} cleanup is uncertain; owned workspace {} is preserved and state cannot be reused",
                active.run_id,
                active.workspace.display()
            );
        }
        let records = directory.join("records");
        match fs::symlink_metadata(&records) {
            Ok(metadata) => {
                if !metadata.is_dir()
                    || metadata.file_type().is_symlink()
                    || metadata.uid() != unsafe { nix::libc::geteuid() }
                    || metadata.mode() & 0o077 != 0
                {
                    bail!("state records directory is not private");
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&records)?;
                fs::set_permissions(&records, fs::Permissions::from_mode(0o700))?;
            }
            Err(error) => return Err(error.into()),
        }
        let mut bytes = 0u64;
        let mut count = 0usize;
        for entry in fs::read_dir(&records)? {
            let entry = entry?;
            count += 1;
            if count > 100000 {
                bail!("state record count budget exhausted");
            }
            let metadata = fs::symlink_metadata(entry.path())?;
            if !metadata.is_file() || metadata.len() > MAX_RECORD_BYTES {
                bail!("state record inventory is invalid");
            }
            bytes = bytes
                .checked_add(metadata.len())
                .context("state byte count overflow")?;
            if bytes > MAX_STATE_BYTES {
                bail!("state disk byte budget exhausted");
            }
        }
        Ok(Self {
            directory,
            reuse: config.reuse,
            _lock: lock,
            active: false,
            bytes: std::cell::Cell::new(bytes),
            records: std::cell::Cell::new(count),
        })
    }
    pub fn begin(&mut self, run_id: &str, workspace: &OwnedDirectory) -> Result<()> {
        let active = Active {
            schema_version: 1,
            run_id: run_id.into(),
            workspace: workspace.path.clone(),
        };
        self.publish_active(&active, storage::atomic_write)
    }
    fn publish_active(
        &mut self,
        active: &Active,
        writer: impl FnOnce(&Path, &[u8]) -> Result<()>,
    ) -> Result<()> {
        let bytes = storage::encode(active, MAX_ACTIVE_BYTES)?;
        self.active = true; // Rename can succeed before directory sync fails; confirmed cleanup still removes the sentinel.
        writer(&self.directory.join("active.json"), &bytes)
    }
    pub fn end(&mut self, cleanup_complete: bool) -> Result<()> {
        if self.active && cleanup_complete {
            match fs::remove_file(self.directory.join("active.json")) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            File::open(&self.directory)?.sync_all()?;
            self.active = false;
        }
        Ok(())
    }
    fn path(&self, input: &str, site: &MutationSite) -> Result<PathBuf> {
        crate::quality::validate_sha256(input)?;
        site.validate()?;
        Ok(self
            .directory
            .join("records")
            .join(format!("{input}-{}.json", site.id)))
    }
    pub fn get(&self, input: &str, site: &MutationSite) -> Result<Option<MutationResult>> {
        if self.reuse != ReusePolicy::DeclaredInputs {
            return Ok(None);
        }
        let path = self.path(input, site)?;
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
            Ok(_) => {}
        }
        let envelope: Envelope =
            serde_json::from_slice(&storage::read_regular(&path, MAX_RECORD_BYTES)?)
                .context("invalid mutation state record")?;
        if envelope.schema_version != 1
            || envelope.checksum != storage::digest(envelope.record.as_bytes())
        {
            bail!("state record version/checksum invalid");
        }
        let record: Record =
            serde_json::from_str(&envelope.record).context("invalid state record payload")?;
        if record.schema_version != 1
            || record.input_digest != input
            || record.mutation_id != site.id
            || !record.cleanup_complete
            || record.result.reused
            || record.result.mutation_id != site.id
            || record.result.location != site.location
            || record.result.operator != site.operator
            || record.result.expected != site.expected
            || record.result.replacement != site.replacement
            || record.request.input_digest != input
            || record.request.mutation_id.as_deref() != Some(&site.id)
        {
            bail!("state record does not agree with the current input/site identity");
        }
        record.response.validate(&record.request)?;
        let classified = record.response.classify(
            record
                .result
                .exit_code
                .context("state record lacks observed exit code")?,
        )?;
        if classified != record.result.outcome
            || !matches!(
                classified,
                MutationOutcome::Killed | MutationOutcome::Survived
            )
            || record.result.tests.as_ref() != Some(&record.response.tests)
            || !record.result.elapsed_ms.is_finite()
            || record.result.elapsed_ms < 0.0
        {
            bail!("state outcome cannot be reused");
        }
        if let Some(context) = &record.result.context
            && (context.before.len() > 256 || context.after.len() > 256)
        {
            bail!("state context exceeds its limit");
        }
        if record
            .result
            .message
            .as_ref()
            .is_some_and(|value| value.len() > 8192)
        {
            bail!("state message exceeds its limit");
        }
        let mut result = record.result;
        result.reused = true;
        Ok(Some(result))
    }
    pub fn put(
        &self,
        input: &str,
        site: &MutationSite,
        result: &MutationResult,
        request: &TestExecutionRequest,
        response: &TestExecutionResult,
    ) -> Result<()> {
        if !matches!(
            result.outcome,
            MutationOutcome::Killed | MutationOutcome::Survived
        ) || result.reused
        {
            return Ok(());
        }
        response.validate(request)?;
        if response.classify(
            result
                .exit_code
                .context("completed record lacks exit code")?,
        )? != result.outcome
        {
            bail!("checkpoint outcome disagrees with test result");
        }
        let record = Record {
            schema_version: 1,
            input_digest: input.into(),
            mutation_id: site.id.clone(),
            cleanup_complete: true,
            result: result.clone(),
            request: request.clone(),
            response: response.clone(),
        };
        let record = String::from_utf8(storage::encode(&record, 16777216)?)?;
        let checksum = storage::digest(record.as_bytes());
        let path = self.path(input, site)?;
        let bytes = storage::encode(
            &Envelope {
                schema_version: 1,
                checksum,
                record,
            },
            MAX_RECORD_BYTES,
        )?;
        let (old, new_record) = match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() => (metadata.len(), false),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (0, true),
            _ => bail!("assigned state record is not regular"),
        };
        let total = self
            .bytes
            .get()
            .checked_sub(old)
            .and_then(|v| v.checked_add(bytes.len() as u64))
            .context("state byte count overflow")?;
        if total > MAX_STATE_BYTES {
            bail!("state disk byte budget exhausted");
        }
        let records = self
            .records
            .get()
            .checked_add(usize::from(new_record))
            .context("state record count overflow")?;
        if records > 100000 {
            bail!("state record count budget exhausted");
        }
        // Reserve conservatively before publication; errors can happen after rename.
        self.bytes.set(total);
        self.records.set(records);
        storage::atomic_write(&path, &bytes)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        facts::MutationOperator,
        result::{ExecutionPhase, TestCompletionReason, TestCounts},
        storage,
    };
    use super::*;
    use crate::quality::SourceLocation;
    fn site() -> MutationSite {
        let mut site = MutationSite {
            id: String::new(),
            location: SourceLocation {
                file: "a.ts".into(),
                start: 10,
                end: 11,
                line: 1,
                end_line: 1,
            },
            owner: None,
            operator: MutationOperator::Boolean,
            expected: "true".into(),
            replacement: "false".into(),
            source_sha256: "a".repeat(64),
        };
        site.location.end = site.location.start + site.expected.len();
        site.id = site.identity();
        site
    }
    #[test]
    fn durable_complete_records_require_identity_checksums_and_exclusive_lock() {
        let root = storage::create_private_directory(&std::env::temp_dir(), "archguard-state-test")
            .unwrap();
        let config = StateConfig {
            directory: root.join("state"),
            reuse: ReusePolicy::DeclaredInputs,
            external_inputs: vec![],
        };
        let mut state = RunState::open(&config, &root, &[]).unwrap();
        assert!(RunState::open(&config, &root, &[]).is_err());
        let site = site();
        let input = "b".repeat(64);
        let request = TestExecutionRequest {
            schema_version: 1,
            request_id: "task-1".into(),
            run_id: "run-1".into(),
            input_digest: input.clone(),
            phase: ExecutionPhase::Mutation,
            mutation_id: Some(site.id.clone()),
            result_path: root.join("result.json"),
            max_result_bytes: 1048576,
        };
        let response = TestExecutionResult {
            schema_version: 1,
            request_id: request.request_id.clone(),
            run_id: request.run_id.clone(),
            input_digest: input.clone(),
            complete: true,
            exit_code: 0,
            reason: TestCompletionReason::Finished,
            tests: TestCounts {
                passed: 1,
                failed: 0,
                skipped: 0,
            },
            failures: vec![],
        };
        let mut result = MutationResult::initial(&site);
        result.outcome = MutationOutcome::Survived;
        result.tests = Some(response.tests.clone());
        result.exit_code = Some(0);
        result.elapsed_ms = 112.28964099999999;
        state
            .put(&input, &site, &result, &request, &response)
            .unwrap();
        assert!(state.get(&input, &site).unwrap().unwrap().reused);
        assert!(state.get(&"c".repeat(64), &site).unwrap().is_none());
        let record = state.path(&input, &site).unwrap();
        let mut corrupt = fs::read(&record).unwrap();
        corrupt[1] = b'x';
        fs::write(&record, corrupt).unwrap();
        assert!(state.get(&input, &site).is_err());
        state
            .put(&input, &site, &result, &request, &response)
            .unwrap();
        state.get(&input, &site).unwrap();
        let owned = OwnedDirectory::create(&root, &[]).unwrap();
        state.begin("run-1", &owned).unwrap();
        state.end(true).unwrap();
        drop(state);
        let state = RunState::open(&config, &root, &[]).unwrap();
        assert!(state.get(&input, &site).unwrap().is_some());
        drop(state);
        drop(owned);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn unresolved_active_state_and_input_aliases_are_preserved() {
        let root = storage::create_private_directory(&std::env::temp_dir(), "archguard-state-test")
            .unwrap();
        let config = StateConfig {
            directory: root.join("state"),
            reuse: ReusePolicy::Off,
            external_inputs: vec![],
        };
        let mut state = RunState::open(&config, &root, &[]).unwrap();
        let mut owned = OwnedDirectory::create(&root, &[]).unwrap();
        state.begin("unfinished", &owned).unwrap();
        state.end(false).unwrap();
        owned.cleanup_allowed = false;
        let workspace = owned.path.clone();
        drop(state);
        assert!(RunState::open(&config, &root, &[]).is_err());
        assert!(workspace.is_dir());
        let inside = StateConfig {
            directory: root.join("forbidden"),
            reuse: ReusePolicy::Off,
            external_inputs: vec![],
        };
        assert!(RunState::open(&inside, &root, &[(root.clone(), String::new())]).is_err());
        assert!(!inside.directory.exists());
        fs::remove_file(config.directory.join("active.json")).unwrap();
        let state = RunState::open(&config, &root, &[]).unwrap();
        drop(state);
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod publication_tests {
    use super::*;
    #[test]
    fn failed_directory_sync_after_active_publication_still_allows_confirmed_cleanup() {
        let root =
            storage::create_private_directory(&std::env::temp_dir(), "archguard-publication-test")
                .unwrap();
        let config = StateConfig {
            directory: root.join("state"),
            reuse: ReusePolicy::Off,
            external_inputs: vec![],
        };
        let mut state = RunState::open(&config, &root, &[]).unwrap();
        let mut owned = OwnedDirectory::create(&root, &[]).unwrap();
        let active = Active {
            schema_version: 1,
            run_id: "run-post-rename-failure".into(),
            workspace: owned.path.clone(),
        };
        assert!(
            state
                .publish_active(&active, |path, bytes| {
                    storage::atomic_write(path, bytes)?;
                    bail!("forced failure after rename")
                })
                .is_err()
        );
        assert!(state.directory.join("active.json").is_file());
        owned.cleanup().unwrap();
        state.end(true).unwrap();
        assert!(!state.directory.join("active.json").exists());
        drop(state);
        RunState::open(&config, &root, &[]).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
