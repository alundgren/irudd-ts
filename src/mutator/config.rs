use crate::mutator::{facts::MutationPlanConfig, storage};
use crate::{
    config::Matcher,
    quality::{
        MAX_CONFIGURATION_BYTES, MIN_REPORT_BYTES, validate_configuration, validate_relative_path,
    },
};
use anyhow::{Context, Result, bail};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{MapAccess, Visitor},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MutatorConfig {
    pub schema_version: u32,
    pub plan: MutationPlanConfig,
    pub execution: ExecutionConfig,
}
impl MutatorConfig {
    pub fn read(path: &Path) -> Result<(Self, PathBuf)> {
        let path = path.canonicalize().context("mutation configuration path")?;
        let config: Self = serde_json::from_slice(&storage::read_regular(
            &path,
            MAX_CONFIGURATION_BYTES as u64,
        )?)
        .context("mutation configuration JSON")?;
        config.validate()?;
        Ok((
            config,
            path.parent()
                .context("configuration has no parent")?
                .to_owned(),
        ))
    }
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1 {
            bail!("unsupported mutation configuration version");
        }
        validate_configuration(self)?;
        self.plan.validate()?;
        self.execution.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionConfig {
    pub command: Vec<String>,
    #[serde(default = "default_working_directory")]
    pub working_directory: String,
    #[serde(default = "default_inherited_environment")]
    pub inherit_environment: Vec<String>,
    #[serde(default, deserialize_with = "unique_environment")]
    pub environment: BTreeMap<String, String>,
    #[serde(default)]
    pub workspace: WorkspaceConfig,
    #[serde(default)]
    pub limits: ExecutionLimits,
    #[serde(default)]
    pub state: Option<StateConfig>,
}
fn default_working_directory() -> String {
    ".".into()
}
fn default_inherited_environment() -> Vec<String> {
    vec!["PATH".into()]
}
impl ExecutionConfig {
    pub fn validate(&self) -> Result<()> {
        validate_configuration(self)?;
        if self.command.is_empty()
            || self.command.len() > 256
            || self
                .command
                .iter()
                .any(|part| part.is_empty() || part.contains('\0'))
        {
            bail!("command must contain bounded nonempty explicit arguments");
        }
        let argument_bytes = self.command.iter().try_fold(0usize, |sum, value| {
            sum.checked_add(value.len())
                .context("command byte count overflow")
        })?;
        if argument_bytes > 65536 {
            bail!("command exceeds argument byte limit");
        }
        if self.working_directory != "." {
            validate_relative_path(&self.working_directory)?;
        }
        if self.inherit_environment.len() > 256 || self.environment.len() > 256 {
            bail!("too many configured environment variables");
        }
        let mut inherited = BTreeSet::new();
        for key in &self.inherit_environment {
            validate_environment_key(key)?;
            if !inherited.insert(key) {
                bail!("duplicate inherited environment key");
            }
        }
        for (key, value) in &self.environment {
            validate_environment_key(key)?;
            if value.contains('\0') {
                bail!("environment values cannot contain NUL");
            }
        }
        self.workspace.validate()?;
        self.limits.validate()?;
        if let Some(state) = &self.state {
            state.validate()?;
        }
        Ok(())
    }
}
fn validate_environment_key(key: &str) -> Result<()> {
    if key.is_empty()
        || key.contains(['=', '\0'])
        || key.len() > 4096
        || key.starts_with("ARCHGUARD_MUTATION_")
        || matches!(key, "HOME" | "TMPDIR" | "TMP" | "TEMP")
    {
        bail!("invalid or reserved environment key");
    }
    Ok(())
}
fn unique_environment<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<BTreeMap<String, String>, D::Error> {
    struct Environment;
    impl<'de> Visitor<'de> for Environment {
        type Value = BTreeMap<String, String>;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("unique environment keys")
        }
        fn visit_map<A: MapAccess<'de>>(
            self,
            mut map: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            let mut values = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, String>()? {
                if values.len() >= 256 || values.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom(
                        "too many or duplicate environment keys",
                    ));
                }
            }
            Ok(values)
        }
    }
    deserializer.deserialize_map(Environment)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceConfig {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub dependencies: Vec<DependencyCopy>,
}
impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            include: vec!["**".into()],
            exclude: vec![".git/**".into()],
            dependencies: vec![],
        }
    }
}
impl WorkspaceConfig {
    pub fn validate(&self) -> Result<()> {
        if self.include.is_empty()
            || self.include.len() + self.exclude.len() > 1024
            || self.dependencies.len() > 256
        {
            bail!("workspace selection or dependencies exceed limits");
        }
        for pattern in self.include.iter().chain(&self.exclude) {
            if pattern.is_empty()
                || pattern.len() > 4096
                || pattern.starts_with('/')
                || pattern.contains(['\\', '\0'])
                || pattern.split('/').any(|part| matches!(part, "." | ".."))
            {
                bail!("invalid workspace pattern");
            }
        }
        Matcher::new(&self.include)?;
        Matcher::new(&self.exclude)?;
        let mut destinations = BTreeSet::new();
        for dependency in &self.dependencies {
            validate_config_path(&dependency.source)?;
            validate_relative_path(&dependency.destination)?;
            if dependency.destination.split('/').any(|part| part == ".git")
                || !destinations.insert(&dependency.destination)
            {
                bail!("unsafe or duplicate dependency destination");
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DependencyCopy {
    pub source: PathBuf,
    pub destination: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StateConfig {
    pub directory: PathBuf,
    #[serde(default)]
    pub reuse: ReusePolicy,
    #[serde(default)]
    pub external_inputs: Vec<PathBuf>,
}
impl StateConfig {
    pub fn validate(&self) -> Result<()> {
        validate_config_path(&self.directory)?;
        if self.external_inputs.len() > 256 {
            bail!("too many declared external inputs");
        }
        for input in &self.external_inputs {
            validate_config_path(input)?;
        }
        Ok(())
    }
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReusePolicy {
    #[default]
    Off,
    DeclaredInputs,
}
fn validate_config_path(path: &Path) -> Result<()> {
    let text = path.to_str().context("configuration paths must be UTF-8")?;
    if text.is_empty() || text.len() > 4096 || text.chars().any(char::is_control) {
        bail!("configuration path is empty or exceeds its limit");
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionLimits {
    pub workers: usize,
    pub max_mutants: usize,
    pub command_timeout_ms: u64,
    pub run_timeout_ms: u64,
    pub max_stdin_bytes: u64,
    pub max_stdout_bytes: u64,
    pub max_stderr_bytes: u64,
    pub max_result_bytes: u64,
    pub max_inventory_bytes: u64,
    pub max_report_bytes: u64,
    pub max_retained_log_bytes: u64,
    pub max_workspace_files: usize,
    pub max_workspace_file_bytes: u64,
    pub max_workspace_bytes: u64,
    pub max_total_workspace_bytes: u64,
    pub max_open_files: u64,
    pub max_cpu_seconds: u64,
    pub max_generated_file_bytes: u64,
    pub max_address_space_bytes: Option<u64>,
}
impl Default for ExecutionLimits {
    fn default() -> Self {
        Self {
            workers: 1,
            max_mutants: 1000,
            command_timeout_ms: 120000,
            run_timeout_ms: 3600000,
            max_stdin_bytes: 65536,
            max_stdout_bytes: 1048576,
            max_stderr_bytes: 65536,
            max_result_bytes: 1048576,
            max_inventory_bytes: 16777216,
            max_report_bytes: 8388608,
            max_retained_log_bytes: 4194304,
            max_workspace_files: 100000,
            max_workspace_file_bytes: 268435456,
            max_workspace_bytes: 2147483648,
            max_total_workspace_bytes: 8589934592,
            max_open_files: 1024,
            max_cpu_seconds: 120,
            max_generated_file_bytes: 67108864,
            max_address_space_bytes: None,
        }
    }
}
impl ExecutionLimits {
    pub fn validate(&self) -> Result<()> {
        macro_rules! limit {
            ($field:ident,$maximum:expr) => {
                if self.$field == 0 || self.$field > $maximum {
                    bail!(concat!(stringify!($field), " is outside supported limits"));
                }
            };
        }
        limit!(workers, 32);
        limit!(max_mutants, 100000);
        limit!(command_timeout_ms, 3600000);
        limit!(run_timeout_ms, 86400000);
        limit!(max_stdin_bytes, 67108864);
        limit!(max_stdout_bytes, 8388608);
        limit!(max_stderr_bytes, 1048576);
        limit!(max_result_bytes, 8388608);
        limit!(max_inventory_bytes, 134217728);
        limit!(max_report_bytes, 134217728);
        limit!(max_retained_log_bytes, 67108864);
        limit!(max_workspace_files, 1000000);
        limit!(max_workspace_file_bytes, 4294967296);
        limit!(max_workspace_bytes, 68719476736);
        limit!(max_total_workspace_bytes, 274877906944);
        limit!(max_open_files, 16384);
        limit!(max_cpu_seconds, 3600);
        limit!(max_generated_file_bytes, 4294967296);
        if self.max_result_bytes < 1024
            || self.max_report_bytes < MIN_REPORT_BYTES as u64
            || self.max_open_files < 32
            || self.max_workspace_file_bytes > self.max_workspace_bytes
            || self.max_workspace_bytes > self.max_total_workspace_bytes
        {
            bail!("execution limits cannot support bounded files or reports");
        }
        for value in [
            self.max_stdin_bytes,
            self.max_stdout_bytes,
            self.max_stderr_bytes,
            self.max_result_bytes,
            self.max_report_bytes,
            self.max_retained_log_bytes,
        ] {
            usize::try_from(value).context("execution byte limit exceeds platform range")?;
        }
        if let Some(bytes) = self.max_address_space_bytes {
            if bytes == 0 || bytes > 1099511627776 {
                bail!("address-space bound outside supported limits");
            }
            #[cfg(not(target_os = "linux"))]
            bail!("hard address-space bounds supported only on Linux");
        }
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            let (_, hard) =
                nix::sys::resource::getrlimit(nix::sys::resource::Resource::RLIMIT_NOFILE)?;
            if self.max_open_files > hard {
                bail!("open-file limit exceeds inherited hard limit");
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        bail!("mutation execution requires Linux or macOS");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::mutator::config::*;
    #[test]
    fn explicit_commands_defaults_and_resource_corrections() {
        assert!(serde_json::from_str::<ExecutionConfig>("{}").is_err());
        let mut config: ExecutionConfig =
            serde_json::from_str(r#"{"command":["node","tests.mjs"]}"#).unwrap();
        config.validate().unwrap();
        assert_eq!(config.limits.max_inventory_bytes, 16777216);
        assert_eq!(config.inherit_environment, vec!["PATH"]);
        config.command.clear();
        assert!(config.validate().is_err());
        config.command = vec!["node".into()];
        config.limits.workers = 33;
        assert!(config.validate().is_err());
        config.limits.workers = 2;
        config.limits.max_inventory_bytes = 134217729;
        assert!(config.validate().is_err());
        config.limits.max_inventory_bytes = 16777216;
        config.validate().unwrap();
    }
    #[test]
    fn duplicate_and_reserved_environment_rejected_without_values() {
        assert!(
            serde_json::from_str::<ExecutionConfig>(
                r#"{"command":["node"],"environment":{"A":"one","A":"two"}}"#
            )
            .is_err()
        );
        let mut config: ExecutionConfig =
            serde_json::from_str(r#"{"command":["node"],"environment":{"HOME":"sample-secret"}}"#)
                .unwrap();
        let error = config.validate().unwrap_err().to_string();
        assert!(!error.contains("sample-secret"));
        config.environment.clear();
        config
            .environment
            .insert("TOKEN".into(), "sample-secret".into());
        config.validate().unwrap();
        config.workspace.dependencies.push(DependencyCopy {
            source: "/tmp/deps".into(),
            destination: "../original".into(),
        });
        assert!(config.validate().is_err());
        config.workspace.dependencies[0].destination = "node_modules".into();
        config.validate().unwrap();
    }
}
