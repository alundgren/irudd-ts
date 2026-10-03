use crate::mutator::{
    config::{ExecutionConfig, ReusePolicy},
    facts::{MutationPlan, MutationSite, SyntaxValidation},
    result::*,
    state::RunState,
    storage,
    workspace::{self, EntryKind, Inputs, OwnedDirectory, WorkspaceLimit},
};
use crate::subprocess::{ChildLimits, CommandLimits, CommandStop, run_command};
use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct CancellationToken {
    flag: CancellationFlag,
}
#[derive(Clone)]
enum CancellationFlag {
    Owned(Arc<AtomicBool>),
    Static(&'static AtomicBool),
}
impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}
impl CancellationToken {
    pub fn new() -> Self {
        Self {
            flag: CancellationFlag::Owned(Arc::new(AtomicBool::new(false))),
        }
    }
    pub fn from_static_flag(flag: &'static AtomicBool) -> Self {
        Self {
            flag: CancellationFlag::Static(flag),
        }
    }
    pub fn cancel(&self) {
        match &self.flag {
            CancellationFlag::Owned(flag) => flag.store(true, Ordering::Relaxed),
            CancellationFlag::Static(flag) => flag.store(true, Ordering::Relaxed),
        }
    }
    pub fn is_cancelled(&self) -> bool {
        match &self.flag {
            CancellationFlag::Owned(flag) => flag.load(Ordering::Relaxed),
            CancellationFlag::Static(flag) => flag.load(Ordering::Relaxed),
        }
    }
}
#[derive(Debug)]
enum Interrupted {
    Cancelled,
    Deadline,
}
impl std::fmt::Display for Interrupted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Cancelled => "mutation run cancelled",
            Self::Deadline => "mutation run deadline reached",
        })
    }
}
impl std::error::Error for Interrupted {}
fn check_interruption(token: &CancellationToken, deadline: Instant) -> Result<()> {
    if token.is_cancelled() {
        return Err(Interrupted::Cancelled.into());
    }
    if Instant::now() >= deadline {
        return Err(Interrupted::Deadline.into());
    }
    Ok(())
}
fn monitor_failure(
    token: &CancellationToken,
    deadline: Instant,
) -> (MutationOutcome, ExecutionProblemKind, &'static str) {
    if token.is_cancelled() {
        (
            MutationOutcome::Cancelled,
            ExecutionProblemKind::Cancellation,
            "test command cancelled during workspace monitoring",
        )
    } else if Instant::now() >= deadline {
        (
            MutationOutcome::TimedOut,
            ExecutionProblemKind::Limit,
            "test command deadline reached during workspace monitoring",
        )
    } else {
        (
            MutationOutcome::ExecutionError,
            ExecutionProblemKind::Limit,
            "shared workspace resource monitor interrupted execution",
        )
    }
}
fn cancelled_command(
    token: &CancellationToken,
    deadline: Instant,
    cleanup_unknown: bool,
) -> (MutationOutcome, ExecutionProblemKind, &'static str) {
    if token.is_cancelled() {
        (
            MutationOutcome::Cancelled,
            ExecutionProblemKind::Cancellation,
            "test command cancelled",
        )
    } else if Instant::now() >= deadline {
        (
            MutationOutcome::TimedOut,
            ExecutionProblemKind::Limit,
            "mutation run deadline reached",
        )
    } else {
        (
            MutationOutcome::ExecutionError,
            if cleanup_unknown {
                ExecutionProblemKind::Cleanup
            } else {
                ExecutionProblemKind::Command
            },
            "test command stopped after another worker encountered a shared execution failure",
        )
    }
}
fn sanitize_diagnostic(text: &str) -> String {
    text.chars()
        .filter(|character| {
            (!character.is_control() || matches!(character, '\n' | '\t'))
                && !matches!(character, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
        .collect()
}
fn diagnostic_redactions<'a>(values: impl Iterator<Item = &'a String>) -> Result<Vec<String>> {
    let mut patterns = std::collections::BTreeSet::new();
    let mut bytes = 0usize;
    for value in values {
        let escaped = serde_json::to_string(value)?;
        for representation in [
            value.as_str(),
            escaped.as_str(),
            &escaped[1..escaped.len() - 1],
        ] {
            let normalized = sanitize_diagnostic(representation);
            if !normalized.is_empty() && !patterns.contains(&normalized) {
                bytes = bytes
                    .checked_add(normalized.len())
                    .context("diagnostic redaction budget overflow")?;
                if bytes > 2 * 1024 * 1024 {
                    return Err(WorkspaceLimit("diagnostic redaction budget exhausted").into());
                }
                patterns.insert(normalized);
            }
        }
    }
    Ok(patterns.into_iter().collect())
}
// Normalize a fixed input prefix before matching literal/escaped values. A
// potentially clipped value suffix is redacted conservatively before display.
fn output_excerpt(bytes: &[u8], redactions: &[impl AsRef<str>]) -> String {
    const EXAMINED_BYTES: usize = 8192;
    const COPIED_BYTES: usize = 2048;
    const DISPLAY_BYTES: usize = 1536;
    let mut examined = bytes.len().min(EXAMINED_BYTES);
    if examined < bytes.len() {
        // Drop an incomplete final UTF-8 sequence rather than converting its
        // clipped prefix into a different character before redaction.
        let mut cursor = 0;
        while cursor < examined {
            match std::str::from_utf8(&bytes[cursor..examined]) {
                Ok(_) => break,
                Err(error) => match error.error_len() {
                    Some(length) => cursor += error.valid_up_to() + length,
                    None => {
                        examined = cursor + error.valid_up_to();
                        break;
                    }
                },
            }
        }
    }
    let normalized = sanitize_diagnostic(&String::from_utf8_lossy(&bytes[..examined]));
    let normalized_bytes = normalized.as_bytes();
    let mut copied = Vec::with_capacity(COPIED_BYTES);
    let mut cursor = 0;
    while cursor < normalized_bytes.len() && copied.len() < COPIED_BYTES {
        let remaining = &normalized_bytes[cursor..];
        if let Some(value) = redactions
            .iter()
            .map(AsRef::as_ref)
            .filter(|value| {
                !value.is_empty()
                    && (remaining.starts_with(value.as_bytes())
                        || value.as_bytes().starts_with(remaining))
            })
            .max_by_key(|value| value.len())
        {
            const REPLACEMENT: &[u8] = b"[redacted]";
            if copied.len() + REPLACEMENT.len() > COPIED_BYTES {
                break;
            }
            copied.extend_from_slice(REPLACEMENT);
            cursor += value.len().min(remaining.len());
        } else {
            copied.push(normalized_bytes[cursor]);
            cursor += 1;
        }
    }
    let decoded = String::from_utf8_lossy(&copied);
    let mut displayed = String::with_capacity(DISPLAY_BYTES);
    let mut display_truncated = false;
    for character in decoded.chars() {
        if displayed.len() + character.len_utf8() > DISPLAY_BYTES {
            display_truncated = true;
            break;
        }
        displayed.push(character);
    }
    if examined < bytes.len() || cursor < normalized_bytes.len() || display_truncated {
        displayed.push_str("\n[output truncated]");
    }
    displayed
}
fn command_diagnostics(stdout: &[u8], stderr: &[u8], redactions: &[impl AsRef<str>]) -> String {
    let mut diagnostics = String::new();
    for (name, bytes) in [("stderr", stderr), ("stdout", stdout)] {
        if !bytes.is_empty() {
            diagnostics.push_str(&format!("\n{name}:\n"));
            diagnostics.push_str(&output_excerpt(bytes, redactions));
        }
    }
    diagnostics
}
fn diagnostic_message(reason: &str, diagnostics: &str) -> String {
    let mut end = reason.len().min(4096);
    while !reason.is_char_boundary(end) {
        end -= 1;
    }
    bounded_message(&format!("{}{}", &reason[..end], diagnostics))
}
fn problem_kind(error: &anyhow::Error, fallback: ExecutionProblemKind) -> ExecutionProblemKind {
    if let Some(problem) = error.downcast_ref::<ClassifiedError>() {
        problem.kind
    } else if error.downcast_ref::<WorkspaceLimit>().is_some() {
        ExecutionProblemKind::Limit
    } else if let Some(reason) = error.downcast_ref::<Interrupted>() {
        match reason {
            Interrupted::Cancelled => ExecutionProblemKind::Cancellation,
            Interrupted::Deadline => ExecutionProblemKind::Limit,
        }
    } else {
        fallback
    }
}
#[derive(Debug)]
struct ClassifiedError {
    kind: ExecutionProblemKind,
    message: String,
}
impl std::fmt::Display for ClassifiedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ClassifiedError {}
fn classified(error: anyhow::Error, fallback: ExecutionProblemKind) -> anyhow::Error {
    ClassifiedError {
        kind: problem_kind(&error, fallback),
        message: bounded_message(&error.to_string()),
    }
    .into()
}
const MAX_EXTERNAL_FILE_BYTES: u64 = 1073741824;
const MAX_EXTERNAL_TOTAL_BYTES: u64 = 8589934592;
struct Prepared {
    program: PathBuf,
    invocation: PathBuf,
    environment: BTreeMap<String, String>,
    redactions: Vec<String>,
    external: Vec<(PathBuf, String, u32)>,
    command_digest: String,
    environment_digest: String,
    product_digest: String,
    resolutions: Vec<workspace::ResolutionEvidence>,
}
impl Prepared {
    fn new(
        config: &ExecutionConfig,
        config_dir: &Path,
        inputs: &Inputs,
        guard: &impl Fn() -> Result<()>,
    ) -> Result<Self> {
        let mut environment = BTreeMap::new();
        for key in &config.inherit_environment {
            if let Some(value) = std::env::var_os(key) {
                environment.insert(
                    key.clone(),
                    value
                        .into_string()
                        .map_err(|_| anyhow::anyhow!("inherited environment must be UTF-8"))?,
                );
            }
        }
        environment.extend(config.environment.clone());
        let environment_bytes = storage::encode(&environment, 65536)
            .context("execution environment exceeds byte budget")?;
        let (program, invocation) = resolve_program(
            &config.command[0],
            &environment,
            &inputs.root.join(&config.working_directory),
        )?;
        let mut external = Vec::new();
        let mut resolutions =
            workspace::resolve_evidence(&invocation, config.limits.max_inventory_bytes, guard)?.1;
        let mut paths = vec![program.clone(), std::env::current_exe()?.canonicalize()?];
        if let Some(state) = &config.state {
            for path in &state.external_inputs {
                let (resolved, evidence) = workspace::resolve_evidence(
                    &workspace::absolute(config_dir, path),
                    config.limits.max_inventory_bytes,
                    guard,
                )
                .context("declared external input unavailable")?;
                resolutions.extend(evidence);
                storage::encoded_size(&resolutions, config.limits.max_inventory_bytes)?;
                paths.push(resolved);
            }
        }
        paths.sort();
        paths.dedup();
        let mut bytes = 0u64;
        let mut metadata_bytes =
            storage::encoded_size(&resolutions, config.limits.max_inventory_bytes)?;
        for path in paths {
            guard()?;
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.is_dir() {
                let mut outside = config.clone();
                outside.workspace = Default::default();
                outside.workspace.exclude.clear();
                outside.state = None;
                outside.limits.max_workspace_file_bytes = MAX_EXTERNAL_FILE_BYTES;
                outside.limits.max_workspace_bytes = MAX_EXTERNAL_TOTAL_BYTES;
                let inventory = Inputs::inventory_external(&path, &outside, config_dir, guard)?;
                bytes = bytes
                    .checked_add(inventory.bytes)
                    .context("external input byte count overflow")?;
                let encoded =
                    storage::encode(&inventory.entries, config.limits.max_inventory_bytes)?;
                metadata_bytes = metadata_bytes
                    .checked_add(encoded.len() as u64)
                    .context("external metadata byte count overflow")?;
                external.push((path, storage::digest(&encoded), inventory.root_mode));
                metadata_bytes = metadata_bytes
                    .checked_add(storage::encoded_size(
                        &inventory.resolutions,
                        config.limits.max_inventory_bytes,
                    )?)
                    .context("external resolution byte count overflow")?;
                if metadata_bytes > config.limits.max_inventory_bytes {
                    return Err(WorkspaceLimit("external metadata budget exhausted").into());
                }
                resolutions.extend(inventory.resolutions);
            } else if metadata.is_file() {
                bytes = bytes
                    .checked_add(metadata.len())
                    .context("external input byte count overflow")?;
                external.push((
                    path.clone(),
                    workspace::hash_file(&path, MAX_EXTERNAL_FILE_BYTES, guard)?,
                    mode(&metadata),
                ));
                metadata_bytes = metadata_bytes
                    .checked_add(path.as_os_str().len() as u64 * 6 + 256)
                    .context("external metadata byte count overflow")?;
            } else {
                bail!("declared external input must be regular or a directory");
            }
            if bytes > MAX_EXTERNAL_TOTAL_BYTES
                || metadata_bytes > config.limits.max_inventory_bytes
            {
                return Err(WorkspaceLimit("external input budget exhausted").into());
            }
        }
        let product = std::env::current_exe()?.canonicalize()?;
        let product_digest = external
            .iter()
            .find(|(path, _, _)| *path == product)
            .context("missing executable identity")?
            .1
            .clone();
        let redactions = diagnostic_redactions(config.command.iter().chain(environment.values()))?;
        Ok(Self {
            program,
            invocation,
            environment,
            redactions,
            external,
            command_digest: storage::digest(&storage::encode(&config.command, 65536)?),
            environment_digest: storage::digest(&environment_bytes),
            product_digest,
            resolutions,
        })
    }
    fn verify(
        &self,
        config: &ExecutionConfig,
        config_dir: &Path,
        inputs: &Inputs,
        guard: &impl Fn() -> Result<()>,
    ) -> Result<()> {
        inputs.verify(config, config_dir, guard)?;
        let now = Self::new(config, config_dir, inputs, guard)?;
        if self.program != now.program
            || self.invocation != now.invocation
            || self.external != now.external
            || self.environment_digest != now.environment_digest
            || self.product_digest != now.product_digest
            || self.resolutions != now.resolutions
        {
            bail!("external execution inputs changed");
        }
        Ok(())
    }
    fn identity(
        &self,
        plan: &MutationPlan,
        config: &ExecutionConfig,
        inputs: &Inputs,
    ) -> Result<String> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Identity<'a> {
            schema_version: u32,
            runner_version: u32,
            product_version: &'static str,
            product_digest: &'a str,
            os: &'static str,
            architecture: &'static str,
            root: &'a Path,
            plan_configuration: &'a crate::mutator::facts::MutationPlanConfig,
            sites: &'a [MutationSite],
            entries: &'a [workspace::Entry],
            root_mode: u32,
            input_resolutions: &'a [workspace::ResolutionEvidence],
            external_resolutions: &'a [workspace::ResolutionEvidence],
            roots: &'a [(PathBuf, String)],
            program: &'a Path,
            invocation: &'a Path,
            external: &'a [(PathBuf, String, u32)],
            command_digest: &'a str,
            environment_digest: &'a str,
            working_directory: &'a str,
            limits: &'a crate::mutator::config::ExecutionLimits,
            workspace: &'a crate::mutator::config::WorkspaceConfig,
        }
        // Dependency source paths are included only in the hash; no configuration/env values are persisted.
        let identity = Identity {
            schema_version: 1,
            runner_version: 1,
            product_version: env!("CARGO_PKG_VERSION"),
            product_digest: &self.product_digest,
            os: std::env::consts::OS,
            architecture: std::env::consts::ARCH,
            root: &inputs.root,
            plan_configuration: &plan.configuration,
            sites: &plan.sites,
            entries: &inputs.entries,
            root_mode: inputs.root_mode,
            input_resolutions: &inputs.resolutions,
            external_resolutions: &self.resolutions,
            roots: &inputs.roots,
            program: &self.program,
            invocation: &self.invocation,
            external: &self.external,
            command_digest: &self.command_digest,
            environment_digest: &self.environment_digest,
            working_directory: &config.working_directory,
            limits: &config.limits,
            workspace: &config.workspace,
        };
        Ok(storage::digest(&storage::encode(
            &identity,
            config
                .limits
                .max_inventory_bytes
                .checked_add(plan.configuration.limits.max_report_bytes as u64)
                .context("identity byte budget overflow")?,
        )?))
    }
    fn command(
        &self,
        config: &ExecutionConfig,
        inputs: &Inputs,
        workspace: &Path,
        task: &Path,
        request: &TestExecutionRequest,
    ) -> Result<Command> {
        let program = relocate(&self.program, inputs, workspace);
        let mut command = Command::new(program);
        use std::os::unix::process::CommandExt;
        command.arg0(relocate(&self.invocation, inputs, workspace));
        for argument in &config.command[1..] {
            let path = Path::new(argument);
            if path.is_absolute() {
                command.arg(relocate(path, inputs, workspace));
            } else {
                command.arg(argument);
            }
        }
        let cwd = workspace.join(&config.working_directory);
        if !cwd.is_dir() {
            bail!("configured working directory was not copied");
        }
        let home = task.join("home");
        let temporary = task.join("tmp");
        fs::create_dir(&home)?;
        fs::create_dir(&temporary)?;
        command
            .current_dir(cwd)
            .env_clear()
            .envs(&self.environment)
            .env("HOME", home)
            .env("TMPDIR", &temporary)
            .env("TMP", &temporary)
            .env("TEMP", temporary)
            .env("ARCHGUARD_MUTATION_REQUEST", task.join("request.json"))
            .env("ARCHGUARD_MUTATION_RESULT", &request.result_path);
        Ok(command)
    }
}
fn mode(metadata: &fs::Metadata) -> u32 {
    use std::os::unix::fs::MetadataExt;
    metadata.mode() & 0o777
}
fn relocate(path: &Path, inputs: &Inputs, destination: &Path) -> PathBuf {
    for (root, relative) in &inputs.roots {
        if let Ok(suffix) = path.strip_prefix(root) {
            return destination.join(relative).join(suffix);
        }
    }
    path.to_owned()
}
fn resolve_program(
    command: &str,
    environment: &BTreeMap<String, String>,
    cwd: &Path,
) -> Result<(PathBuf, PathBuf)> {
    let path = Path::new(command);
    let candidates = if path.components().count() > 1 || path.is_absolute() {
        vec![if path.is_absolute() {
            path.to_owned()
        } else {
            cwd.join(path)
        }]
    } else {
        std::env::split_paths(
            environment
                .get("PATH")
                .context("bare command requires explicitly inherited/configured PATH")?,
        )
        .map(|parent| {
            if parent.is_absolute() {
                parent.join(path)
            } else {
                cwd.join(parent).join(path)
            }
        })
        .collect()
    };
    for candidate in candidates {
        if let Ok(path) = candidate.canonicalize() {
            let metadata = fs::metadata(&path)?;
            if metadata.is_file() && mode(&metadata) & 0o111 != 0 {
                return Ok((path, candidate));
            }
        }
    }
    bail!("explicit test command executable unavailable")
}
fn write_mutated_source(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let parent = path.parent().context("mutant source has no parent")?;
    let directory_mode = fs::symlink_metadata(parent)?.permissions().mode();
    let file_mode = fs::symlink_metadata(path)?.permissions().mode();
    fs::set_permissions(parent, fs::Permissions::from_mode(directory_mode | 0o300))?;
    let written = storage::atomic_write(path, bytes);
    let restored = fs::set_permissions(parent, fs::Permissions::from_mode(directory_mode));
    written?;
    restored?;
    fs::set_permissions(path, fs::Permissions::from_mode(file_mode))?;
    Ok(())
}
struct TaskResult {
    result: MutationResult,
    request: Option<TestExecutionRequest>,
    response: Option<TestExecutionResult>,
    problem: Option<ExecutionProblem>,
    cleanup_complete: bool,
}
struct TaskContext<'a> {
    config: &'a ExecutionConfig,
    prepared: &'a Prepared,
    original: &'a Inputs,
    template: &'a Inputs,
    parent: &'a Path,
    run_id: &'a str,
    input_digest: &'a str,
    token: &'a CancellationToken,
    deadline: Instant,
    analysis_limits: &'a crate::quality::AnalysisLimits,
    validation_gate: &'a Mutex<()>,
    stop: &'a AtomicBool,
    cleanup_unknown: &'a AtomicBool,
}
impl TaskContext<'_> {
    fn execute(&self, site: Option<&MutationSite>) -> TaskResult {
        let start = Instant::now();
        let mut result = site
            .map(MutationResult::initial)
            .unwrap_or_else(|| MutationResult {
                mutation_id: String::new(),
                location: crate::quality::SourceLocation {
                    file: "baseline.ts".into(),
                    start: 0,
                    end: 1,
                    line: 1,
                    end_line: 1,
                },
                operator: crate::mutator::facts::MutationOperator::Boolean,
                expected: String::new(),
                replacement: String::new(),
                context: None,
                outcome: MutationOutcome::NotRun,
                reused: false,
                tests: None,
                exit_code: None,
                elapsed_ms: 0.0,
                message: None,
            });
        let mut request = None;
        let mut response = None;
        let mut cleanup_complete = true;
        let mut diagnostics = String::new();
        let mut kind = ExecutionProblemKind::Workspace;
        let task = storage::create_private_directory(self.parent, "task");
        let mut owned = match task {
            Ok(path) => match OwnedDirectory::from_path(path) {
                Ok(owned) => owned,
                Err(error) => {
                    return TaskResult {
                        result,
                        request,
                        response,
                        problem: Some(ExecutionProblem {
                            kind,
                            mutation_id: site.map(|site| site.id.clone()),
                            file: site.map(|site| site.location.file.clone()),
                            message: bounded_message(&error.to_string()),
                        }),
                        cleanup_complete: false,
                    };
                }
            },
            Err(error) => {
                return TaskResult {
                    result,
                    request,
                    response,
                    problem: Some(ExecutionProblem {
                        kind,
                        mutation_id: site.map(|site| site.id.clone()),
                        file: site.map(|site| site.location.file.clone()),
                        message: bounded_message(&error.to_string()),
                    }),
                    cleanup_complete: true,
                };
            }
        };
        let task_result = (|| -> Result<()> {
            let guard = || check_interruption(self.token, self.deadline);
            guard()?;
            let workspace = owned.path.join("workspace");
            self.template.copy_to(&workspace, &guard)?;
            if let Some(site) = site {
                let path = workspace.join(&site.location.file);
                let source = String::from_utf8(storage::read_regular(
                    &path,
                    self.config.limits.max_workspace_file_bytes,
                )?)
                .context("mutation source is not UTF-8")?;
                result.source_context(&source);
                kind = ExecutionProblemKind::InvalidPlan;
                let mutated = crate::mutator::apply_edit(&source, site)
                    .map_err(|error| anyhow::anyhow!(error.message))?;
                let validation = {
                    let _permit = self
                        .validation_gate
                        .lock()
                        .map_err(|_| anyhow::anyhow!("mutant validation coordinator failed"))?;
                    guard()?;
                    crate::mutator::validate_mutant(
                        &site.location.file,
                        &mutated,
                        self.analysis_limits,
                    )?
                };
                match validation {
                    SyntaxValidation::Valid => {}
                    SyntaxValidation::InvalidSyntax { diagnostics } => {
                        result.outcome = MutationOutcome::InvalidMutant;
                        result.message = diagnostics
                            .first()
                            .map(|problem| bounded_message(&problem.message));
                        return Ok(());
                    }
                    SyntaxValidation::Incomplete { problems } => {
                        kind = if problems.iter().any(|problem| problem.limit.is_some()) {
                            ExecutionProblemKind::Limit
                        } else {
                            ExecutionProblemKind::Command
                        };
                        bail!("mutant syntax validation is incomplete");
                    }
                }
                if mutated.len() as u64 > self.config.limits.max_workspace_file_bytes {
                    kind = ExecutionProblemKind::Limit;
                    bail!("mutant exceeds workspace file byte budget");
                }
                kind = ExecutionProblemKind::Workspace;
                write_mutated_source(&path, mutated.as_bytes())?;
            }
            kind = ExecutionProblemKind::Limit;
            workspace::disk_usage(self.parent, &self.config.limits, &guard)?;
            let protocol = TestExecutionRequest {
                schema_version: 1,
                request_id: storage::unique_id(),
                run_id: self.run_id.into(),
                input_digest: self.input_digest.into(),
                phase: if site.is_some() {
                    ExecutionPhase::Mutation
                } else {
                    ExecutionPhase::Baseline
                },
                mutation_id: site.map(|site| site.id.clone()),
                result_path: owned.path.join("result.json"),
                max_result_bytes: self.config.limits.max_result_bytes,
            };
            protocol.validate()?;
            let payload = storage::encode(
                &protocol,
                MAX_REQUEST_BYTES.min(self.config.limits.max_stdin_bytes),
            )?;
            storage::atomic_write(&owned.path.join("request.json"), &payload)?;
            kind = ExecutionProblemKind::Command;
            let command = self.prepared.command(
                self.config,
                self.original,
                &workspace,
                &owned.path,
                &protocol,
            )?;
            let limits = CommandLimits {
                deadline: self.deadline.min(
                    Instant::now() + Duration::from_millis(self.config.limits.command_timeout_ms),
                ),
                stdin_bytes: self.config.limits.max_stdin_bytes as usize,
                stdout_bytes: self.config.limits.max_stdout_bytes as usize,
                stderr_bytes: self.config.limits.max_stderr_bytes as usize,
                resources: ChildLimits {
                    core_bytes: Some(0),
                    cpu_seconds: Some(self.config.limits.max_cpu_seconds),
                    file_bytes: Some(self.config.limits.max_generated_file_bytes),
                    open_files: Some(self.config.limits.max_open_files),
                    address_space_bytes: self.config.limits.max_address_space_bytes,
                },
            };
            let raw = run_command(
                command,
                &payload,
                &limits,
                || self.token.is_cancelled() || self.stop.load(Ordering::Relaxed),
                || {
                    let command_guard = || check_interruption(self.token, limits.deadline);
                    workspace::disk_usage(self.parent, &self.config.limits, &command_guard)?;
                    let mut local = self.config.limits.clone();
                    local.max_total_workspace_bytes = local.max_workspace_bytes;
                    local.workers = 0;
                    workspace::disk_usage(&owned.path, &local, &command_guard)?;
                    Ok(())
                },
            )?;
            diagnostics = command_diagnostics(&raw.stdout, &raw.stderr, &self.prepared.redactions);
            cleanup_complete = raw.cleanup_complete;
            owned.cleanup_allowed = cleanup_complete;
            if !cleanup_complete {
                self.cleanup_unknown.store(true, Ordering::Relaxed);
                kind = ExecutionProblemKind::Cleanup;
                bail!(
                    "test process cleanup is uncertain; workspace preserved for run {} at {}; transport: {:?}",
                    self.run_id,
                    owned.path.display(),
                    raw.stop
                );
            }
            result.exit_code = raw.status.and_then(|status| status.code());
            match raw.stop {
                CommandStop::Completed => {}
                CommandStop::TimedOut => {
                    result.outcome = MutationOutcome::TimedOut;
                    kind = ExecutionProblemKind::Limit;
                    bail!("test command deadline reached");
                }
                CommandStop::Cancelled => {
                    let (outcome, problem, message) = cancelled_command(
                        self.token,
                        self.deadline,
                        self.cleanup_unknown.load(Ordering::Relaxed),
                    );
                    result.outcome = outcome;
                    kind = problem;
                    bail!(message);
                }
                CommandStop::Monitor(_) => {
                    let (outcome, problem, message) = monitor_failure(self.token, limits.deadline);
                    if outcome != MutationOutcome::TimedOut || Instant::now() >= self.deadline {
                        self.stop.store(true, Ordering::Relaxed);
                    }
                    result.outcome = outcome;
                    kind = problem;
                    bail!(message);
                }
                CommandStop::OutputLimit(_) => {
                    kind = ExecutionProblemKind::Limit;
                    bail!("test command output byte limit reached");
                }
                CommandStop::Io(_) | CommandStop::Cleanup(_) => {
                    bail!("test command transport/cleanup incomplete")
                }
            }
            if !raw.stdin_complete {
                bail!("test command did not consume the request");
            }
            kind = ExecutionProblemKind::Protocol;
            use std::os::unix::process::ExitStatusExt;
            let observed = result.exit_code.ok_or_else(|| {
                anyhow::anyhow!(
                    "test command terminated without an exit code (signal {:?})",
                    raw.status.and_then(|status| status.signal())
                )
            })?;
            let reported = TestExecutionResult::read(&protocol.result_path, &protocol)?;
            result.outcome = reported.classify(observed)?;
            result.tests = Some(reported.tests.clone());
            result.message = reported
                .failures
                .first()
                .map(|failure| bounded_message(&failure.message));
            request = Some(protocol);
            response = Some(reported);
            Ok(())
        })();
        result.elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        let mut problem = task_result.err().map(|error| {
            kind = problem_kind(&error, kind);
            if result.outcome == MutationOutcome::NotRun {
                result.outcome = match error.downcast_ref::<Interrupted>() {
                    Some(Interrupted::Cancelled) => MutationOutcome::Cancelled,
                    Some(Interrupted::Deadline) => MutationOutcome::TimedOut,
                    None => MutationOutcome::ExecutionError,
                };
            }
            let message = diagnostic_message(&error.to_string(), &diagnostics);
            result.message = Some(message.clone());
            ExecutionProblem {
                kind,
                mutation_id: site.map(|site| site.id.clone()),
                file: site.map(|site| site.location.file.clone()),
                message,
            }
        });
        if owned.cleanup_allowed
            && let Err(error) = owned.cleanup()
        {
            cleanup_complete = false;
            self.cleanup_unknown.store(true, Ordering::Relaxed);
            owned.cleanup_allowed = false;
            result.outcome = MutationOutcome::ExecutionError;
            let message = diagnostic_message(&error.to_string(), &diagnostics);
            result.message = Some(message.clone());
            problem = Some(ExecutionProblem {
                kind: ExecutionProblemKind::Cleanup,
                mutation_id: site.map(|site| site.id.clone()),
                file: site.map(|site| site.location.file.clone()),
                message,
            });
        }
        TaskResult {
            result,
            request,
            response,
            problem,
            cleanup_complete,
        }
    }
}

pub fn run(
    plan: &MutationPlan,
    config: &ExecutionConfig,
    config_dir: &Path,
    token: &CancellationToken,
) -> Result<MutationReport> {
    let deadline = Instant::now()
        .checked_add(Duration::from_millis(config.limits.run_timeout_ms))
        .context("mutation timeout exceeds clock range")?;
    run_until(plan, config, config_dir, token, deadline)
}
pub fn run_until(
    plan: &MutationPlan,
    config: &ExecutionConfig,
    config_dir: &Path,
    token: &CancellationToken,
    deadline: Instant,
) -> Result<MutationReport> {
    config.validate()?;
    plan.validate()?;
    let config_dir = config_dir
        .canonicalize()
        .context("configuration directory unavailable")?;
    let start = Instant::now();
    let deadline = deadline.min(
        start
            .checked_add(Duration::from_millis(config.limits.run_timeout_ms))
            .context("mutation timeout exceeds clock range")?,
    );
    let reuse = config
        .state
        .as_ref()
        .map_or(ReusePolicy::Off, |state| state.reuse);
    let summary=ExecutionSummary{limits:config.limits.clone(),workspace:WorkspaceSummary{include:config.workspace.include.clone(),exclude:config.workspace.exclude.clone(),dependency_destinations:config.workspace.dependencies.iter().map(|dependency|dependency.destination.clone()).collect()},reuse,reuse_reason:Some(if reuse==ReusePolicy::DeclaredInputs{"reuse relies on explicitly declared deterministic inputs; baseline always runs fresh"}else{"result reuse is disabled"}.into())};
    let mut report = MutationReport::initial(plan, summary);
    let mut evidence_budget = EvidenceBudget::new(&report);
    let mut owned = None;
    let mut state = None;
    let cleanup_unknown = AtomicBool::new(false);
    let stop = AtomicBool::new(false);
    let operation = (|| -> Result<()> {
        let guard = || check_interruption(token, deadline);
        guard()?;
        if !report.problems.is_empty() {
            return Ok(());
        }
        if !plan.complete {
            report.problem(
                ExecutionProblemKind::InvalidPlan,
                None,
                None,
                "mutation plan is incomplete",
            );
            return Ok(());
        }
        if plan.sites.len() > config.limits.max_mutants {
            report.problem(
                ExecutionProblemKind::Limit,
                None,
                None,
                "mutation count exceeds configured execution limit",
            );
            return Ok(());
        }
        if storage::encode(&report, config.limits.max_report_bytes).is_err() {
            report.problem(
                ExecutionProblemKind::Limit,
                None,
                None,
                "planned evidence exceeds execution report budget",
            );
            return Ok(());
        }
        let regenerated =
            crate::mutator::plan_guarded(Path::new(&plan.root), &plan.configuration, &guard)?;
        guard()?;
        if !regenerated.complete
            || storage::encode(plan, plan.configuration.limits.max_report_bytes as u64)?
                != storage::encode(
                    &regenerated,
                    plan.configuration.limits.max_report_bytes as u64,
                )?
        {
            report.problem(
                ExecutionProblemKind::InvalidPlan,
                None,
                None,
                "plan does not match the complete current runtime-expression inventory",
            );
            return Ok(());
        }
        let inputs = Inputs::inventory(Path::new(&plan.root), config, &config_dir, &guard)?;
        for file in &plan.files {
            let entry = inputs
                .entries
                .iter()
                .find(|entry| entry.path == file.path)
                .context("selected mutation source was not copied by workspace configuration")?;
            if entry.kind != EntryKind::File
                || entry.bytes != file.bytes as u64
                || entry.digest != file.sha256
            {
                bail!("selected mutation source differs from planned bytes");
            }
        }
        let prepared = Prepared::new(config, &config_dir, &inputs, &guard)?;
        report.input_digest = prepared.identity(plan, config, &inputs)?;
        let log_reservation = config
            .limits
            .max_stdout_bytes
            .checked_add(config.limits.max_stderr_bytes)
            .and_then(|bytes| bytes.checked_mul(config.limits.workers as u64))
            .context("log reservation overflow")?;
        if log_reservation > config.limits.max_retained_log_bytes {
            report.problem(ExecutionProblemKind::Limit,None,None,"configured workers/stream caps exceed aggregate retained log budget; reduce workers/caps or increase budget");
            return Ok(());
        }
        let copies = inputs
            .bytes
            .checked_mul(config.limits.workers as u64 + 1)
            .context("worker disk reservation overflow")?;
        if copies > config.limits.max_total_workspace_bytes {
            report.problem(
                ExecutionProblemKind::Limit,
                None,
                None,
                "template and configured workers exceed aggregate workspace reservation",
            );
            return Ok(());
        }
        if let Some(state_config) = &config.state {
            let candidate = RunState::open(state_config, &config_dir, &inputs.roots)
                .map_err(|error| classified(error, ExecutionProblemKind::State))?;
            for (external, _, _) in &prepared.external {
                if external.starts_with(&candidate.directory)
                    || candidate.directory.starts_with(external)
                {
                    bail!("state directory overlaps a declared execution input");
                }
            }
            state = Some(candidate);
        }
        let parent = state
            .as_ref()
            .map_or_else(std::env::temp_dir, |state| state.directory.clone());
        owned = Some(OwnedDirectory::create(&parent, &inputs.roots)?);
        let parent = &owned.as_ref().context("missing owned workspace")?.path;
        let run_id = storage::unique_id();
        if let Some(state) = &mut state {
            state
                .begin(&run_id, owned.as_ref().context("missing owned workspace")?)
                .map_err(|error| classified(error, ExecutionProblemKind::State))?;
        }
        let template_path = parent.join("template");
        inputs.copy_to(&template_path, &guard)?;
        prepared
            .verify(config, &config_dir, &inputs, &guard)
            .map_err(|error| classified(error, ExecutionProblemKind::InputChanged))?;
        let mut template = inputs.clone();
        for entry in &mut template.entries {
            entry.source = template_path.join(&entry.path);
        }
        let input_digest = report.input_digest.clone();
        let validation_gate = Mutex::new(());
        let context = TaskContext {
            config,
            prepared: &prepared,
            original: &inputs,
            template: &template,
            parent,
            run_id: &run_id,
            input_digest: &input_digest,
            token,
            deadline,
            analysis_limits: &plan.configuration.limits,
            validation_gate: &validation_gate,
            stop: &stop,
            cleanup_unknown: &cleanup_unknown,
        };
        let baseline = context.execute(None);
        report.baseline = BaselineResult {
            outcome: match baseline.result.outcome {
                MutationOutcome::Survived => BaselineOutcome::Passed,
                MutationOutcome::Killed => BaselineOutcome::Failed,
                MutationOutcome::TimedOut => BaselineOutcome::TimedOut,
                MutationOutcome::Cancelled => BaselineOutcome::Cancelled,
                _ => BaselineOutcome::ExecutionError,
            },
            tests: baseline.result.tests,
            exit_code: baseline.result.exit_code,
            elapsed_ms: baseline.result.elapsed_ms,
            message: baseline.result.message,
        };
        if let Some(problem) = baseline.problem {
            report.problem(
                problem.kind,
                problem.mutation_id,
                problem.file,
                &problem.message,
            );
        }
        if report.baseline.outcome != BaselineOutcome::Passed {
            report.problem(
                ExecutionProblemKind::Baseline,
                None,
                None,
                "fresh baseline did not complete with at least one passing test and no failures",
            );
            return Ok(());
        }
        prepared
            .verify(config, &config_dir, &inputs, &guard)
            .map_err(|error| classified(error, ExecutionProblemKind::InputChanged))?;
        evidence_budget = EvidenceBudget::new(&report);
        let mut tasks = Vec::new();
        let mut rejected_cache = 0usize;
        for (index, site) in plan.sites.iter().enumerate() {
            guard()?;
            if let Some(state) = &state {
                match state.get(&report.input_digest, site) {
                    Ok(Some(saved)) => {
                        if !report.install_result(index, saved, &mut evidence_budget) {
                            report.problem(
                                ExecutionProblemKind::Limit,
                                None,
                                None,
                                "retained result evidence exceeds report byte budget",
                            );
                            return Ok(());
                        }
                        continue;
                    }
                    Ok(None) => {}
                    Err(_) => {
                        rejected_cache += 1;
                    }
                }
            }
            tasks.push(index);
        }
        if rejected_cache > 0 {
            report.execution.reuse_reason = Some(format!(
                "Rejected {rejected_cache} invalid cache records; those mutations execute freshly under declared-input trust"
            ));
        }
        evidence_budget = EvidenceBudget::new(&report);
        let next = AtomicUsize::new(0);
        let (send, receive) = mpsc::sync_channel(config.limits.workers);
        std::thread::scope(|scope| {
            for _ in 0..config.limits.workers.min(tasks.len()) {
                let send = send.clone();
                let context = &context;
                let next = &next;
                let tasks = &tasks;
                let launch = std::thread::Builder::new()
                    .name("archguard-mutator-worker".into())
                    .spawn_scoped(scope, move || {
                        while !context.stop.load(Ordering::Relaxed)
                            && !context.token.is_cancelled()
                            && Instant::now() < context.deadline
                        {
                            let slot = next.fetch_add(1, Ordering::Relaxed);
                            let Some(&index) = tasks.get(slot) else {
                                break;
                            };
                            let task = context.execute(Some(&plan.sites[index]));
                            if task.problem.as_ref().is_some_and(|problem| {
                                matches!(
                                    problem.kind,
                                    ExecutionProblemKind::Cleanup
                                        | ExecutionProblemKind::Cancellation
                                        | ExecutionProblemKind::InputChanged
                                        | ExecutionProblemKind::InvalidPlan
                                )
                            }) {
                                context.stop.store(true, Ordering::Relaxed);
                            }
                            if send.send((index, task)).is_err() {
                                break;
                            }
                        }
                    });
                if let Err(error) = launch {
                    stop.store(true, Ordering::Relaxed);
                    report.problem(
                        ExecutionProblemKind::Command,
                        None,
                        None,
                        &format!("mutation worker could not start: {error}"),
                    );
                    break;
                }
            }
            drop(send);
            for (index, task) in receive {
                let retained = report.install_result(index, task.result, &mut evidence_budget);
                if !retained {
                    stop.store(true, Ordering::Relaxed);
                    report.problem(
                        ExecutionProblemKind::Limit,
                        Some(plan.sites[index].id.clone()),
                        None,
                        "retained result evidence exceeds report byte budget",
                    );
                }
                if let Some(problem) = task.problem {
                    report.problem(
                        problem.kind,
                        problem.mutation_id,
                        problem.file,
                        &problem.message,
                    );
                }
                if task.cleanup_complete
                    && matches!(
                        report.results[index].outcome,
                        MutationOutcome::Killed | MutationOutcome::Survived
                    )
                {
                    match prepared.verify(config, &config_dir, &inputs, &guard) {
                        Ok(()) => {
                            if let (Some(state), Some(request), Some(response)) =
                                (&state, task.request, task.response)
                                && let Err(error) = state.put(
                                    &report.input_digest,
                                    &plan.sites[index],
                                    &report.results[index],
                                    &request,
                                    &response,
                                )
                            {
                                stop.store(true, Ordering::Relaxed);
                                report.problem(
                                    ExecutionProblemKind::State,
                                    Some(plan.sites[index].id.clone()),
                                    None,
                                    &error.to_string(),
                                );
                            }
                        }
                        Err(error) => {
                            stop.store(true, Ordering::Relaxed);
                            report.problem(
                                problem_kind(&error, ExecutionProblemKind::InputChanged),
                                Some(plan.sites[index].id.clone()),
                                None,
                                &error.to_string(),
                            );
                        }
                    }
                }
            }
        });
        guard()?;
        prepared
            .verify(config, &config_dir, &inputs, &guard)
            .map_err(|error| classified(error, ExecutionProblemKind::InputChanged))?;
        Ok(())
    })();
    if let Err(error) = operation {
        report.problem(
            problem_kind(&error, ExecutionProblemKind::Workspace),
            None,
            None,
            &error.to_string(),
        );
    }
    if let Some(owned) = &mut owned {
        owned.cleanup_allowed = !cleanup_unknown.load(Ordering::Relaxed);
        if owned.cleanup_allowed
            && let Err(error) = owned.cleanup()
        {
            owned.cleanup_allowed = false;
            report.problem(
                ExecutionProblemKind::Cleanup,
                None,
                None,
                &error.to_string(),
            );
        }
        if !owned.cleanup_allowed {
            report.problem(
                ExecutionProblemKind::Cleanup,
                None,
                None,
                &format!(
                    "owned workspace preserved because cleanup was not confirmed: {}",
                    owned.path.display()
                ),
            );
        }
        if let Some(state) = &mut state
            && let Err(error) = state.end(owned.cleanup_allowed)
        {
            report.problem(ExecutionProblemKind::State, None, None, &error.to_string());
        }
    }
    report.omit_results(&evidence_budget);
    report.finish(start.elapsed().as_secs_f64() * 1000.0)?;
    report.count_omitted(&evidence_budget);
    Ok(report)
}

#[cfg(test)]
mod monitor_tests {
    use crate::mutator::run::*;
    use std::cell::Cell;

    #[test]
    fn peer_failure_stop_is_distinct_from_requested_cancellation_and_deadline() {
        let token = CancellationToken::new();
        let future = Instant::now() + Duration::from_secs(30);
        let (outcome, kind, _) = cancelled_command(&token, future, true);
        assert_eq!(outcome, MutationOutcome::ExecutionError);
        assert_eq!(kind, ExecutionProblemKind::Cleanup);
        let (outcome, kind, _) = cancelled_command(&token, future, false);
        assert_eq!(outcome, MutationOutcome::ExecutionError);
        assert_eq!(kind, ExecutionProblemKind::Command);
        let (outcome, kind, _) = cancelled_command(&token, Instant::now(), false);
        assert_eq!(outcome, MutationOutcome::TimedOut);
        assert_eq!(kind, ExecutionProblemKind::Limit);
        token.cancel();
        let (outcome, kind, _) = cancelled_command(&token, future, true);
        assert_eq!(outcome, MutationOutcome::Cancelled);
        assert_eq!(kind, ExecutionProblemKind::Cancellation);
    }

    #[test]
    fn command_excerpt_bounds_redacts_boundary_values_and_sanitizes_output() {
        let secret = "literal-sensitive-value";
        let mut bytes = vec![b'x'; 1528];
        bytes.extend_from_slice(secret.as_bytes());
        bytes.extend_from_slice(&vec![b'z'; 100_000]);
        let excerpt = output_excerpt(&bytes, &["literal", secret]);
        assert!(!excerpt.contains("sensitive"));
        assert!(excerpt.contains("[output truncated]"));
        assert!(excerpt.len() <= 1600);
        let boundary = output_excerpt(b"prefix literal-sensitive", &[secret]);
        assert_eq!(boundary, "prefix [redacted]");
        let escaped_secret = "sensitive\n\"value\\tail".to_owned();
        let patterns = diagnostic_redactions(std::iter::once(&escaped_secret)).unwrap();
        let escaped = serde_json::to_string(&escaped_secret).unwrap();
        assert_eq!(output_excerpt(escaped.as_bytes(), &patterns), "[redacted]");
        let interrupted = escaped_secret.replace("value", "va\u{0}l\u{202e}ue");
        assert_eq!(
            output_excerpt(interrupted.as_bytes(), &patterns),
            "[redacted]"
        );
        let long_secret = "q".repeat(20_000);
        let long_patterns = diagnostic_redactions(std::iter::once(&long_secret)).unwrap();
        let long_excerpt = output_excerpt(long_secret.as_bytes(), &long_patterns);
        assert_eq!(long_excerpt, "[redacted]\n[output truncated]");
        let controls = output_excerpt(b"normal\x1b[31m\x00\xff\nnext\tline", &[] as &[&str]);
        assert!(!controls.contains(['\x1b', '\0']));
        assert!(controls.contains("normal[31m"));
        assert!(controls.contains('\u{fffd}'));
        assert!(controls.contains("\nnext\tline"));
        let message = diagnostic_message(
            &"r".repeat(100_000),
            &command_diagnostics(&vec![b'a'; 100_000], &vec![b'b'; 100_000], &[] as &[&str]),
        );
        assert!(message.len() <= MAX_MESSAGE_BYTES);
        assert!(message.starts_with('r'));
        assert!(message.contains("stderr:"));
        assert!(message.contains("stdout:"));
    }

    #[test]
    fn interrupted_monitor_traversal_preserves_cancellation_and_resource_controls() {
        let directory = tempfile::tempdir().unwrap();
        for index in 0..40 {
            fs::create_dir(directory.path().join(format!("entry-{index}"))).unwrap();
        }
        for mode in ["cancel", "deadline", "resource"] {
            let token = CancellationToken::new();
            let deadline = Instant::now() + Duration::from_secs(2);
            let limits = CommandLimits {
                deadline,
                stdin_bytes: 1024,
                stdout_bytes: 1024,
                stderr_bytes: 1024,
                resources: ChildLimits::default(),
            };
            let mut workspace_limits = crate::mutator::config::ExecutionLimits::default();
            if mode == "resource" {
                workspace_limits.max_inventory_bytes = 1024;
            }
            let visits = Cell::new(0);
            let mut command = Command::new("sh");
            command.args(["-c", "while :; do :; done"]);
            let output = run_command(
                command,
                b"",
                &limits,
                || false,
                || {
                    workspace::disk_usage(directory.path(), &workspace_limits, &|| {
                        visits.set(visits.get() + 1);
                        if visits.get() == 5 {
                            if mode == "cancel" {
                                token.cancel();
                            }
                            if mode == "deadline" {
                                std::thread::sleep(
                                    deadline.saturating_duration_since(Instant::now())
                                        + Duration::from_millis(1),
                                );
                            }
                        }
                        check_interruption(&token, deadline)
                    })?;
                    Ok(())
                },
            )
            .unwrap();
            assert!(matches!(output.stop, CommandStop::Monitor(_)));
            assert!(output.cleanup_complete);
            let (outcome, problem, _) = monitor_failure(&token, deadline);
            if mode == "cancel" {
                assert_eq!(outcome, MutationOutcome::Cancelled);
                assert_eq!(problem, ExecutionProblemKind::Cancellation);
            } else if mode == "deadline" {
                assert_eq!(outcome, MutationOutcome::TimedOut);
                assert_eq!(problem, ExecutionProblemKind::Limit);
            } else {
                assert_eq!(outcome, MutationOutcome::ExecutionError);
                assert_eq!(problem, ExecutionProblemKind::Limit);
            }
        }
        let (outcome, problem, _) = monitor_failure(&CancellationToken::new(), Instant::now());
        assert_eq!(outcome, MutationOutcome::TimedOut);
        assert_eq!(problem, ExecutionProblemKind::Limit);
    }
}
