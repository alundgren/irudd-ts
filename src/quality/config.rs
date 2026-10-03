use crate::config::Matcher;
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::io::{self, Write};

pub const MAX_CONFIGURATION_BYTES: usize = 16 * 1024;
pub const MIN_REPORT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceSelection {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

impl Default for SourceSelection {
    fn default() -> Self {
        Self {
            include: ["**/*.ts", "**/*.tsx", "**/*.mts", "**/*.cts"]
                .map(String::from)
                .into(),
            exclude: vec![],
        }
    }
}

impl SourceSelection {
    pub fn validate(&self) -> Result<()> {
        validate_configuration(self)?;
        if self.include.is_empty() {
            bail!("include must select at least one file pattern");
        }
        for pattern in self.include.iter().chain(&self.exclude) {
            if pattern.is_empty()
                || pattern.starts_with('/')
                || pattern.contains(['\\', ':'])
                || pattern.chars().any(char::is_control)
                || pattern.split('/').any(|part| part == ".." || part == ".")
            {
                bail!("source patterns must use relative forward-slash paths");
            }
        }
        Matcher::new(&self.include)?;
        Matcher::new(&self.exclude)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalysisLimits {
    pub max_file_bytes: usize,
    pub max_total_bytes: usize,
    pub max_files: usize,
    pub max_discovery_entries: usize,
    pub max_raw_units: usize,
    pub max_delimiter_depth: usize,
    pub max_nodes_per_file: usize,
    pub max_nodes: usize,
    pub max_candidates: usize,
    pub max_comparisons: usize,
    pub max_comparison_entries: usize,
    pub max_pairs: usize,
    pub max_sites: usize,
    pub workers: usize,
    pub max_report_bytes: usize,
    pub max_name_bytes: usize,
    pub max_problem_bytes: usize,
    pub max_excerpt_bytes: usize,
}

impl Default for AnalysisLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 1024 * 1024,
            max_total_bytes: 32 * 1024 * 1024,
            max_files: 2_000,
            max_discovery_entries: 100_000,
            max_raw_units: 8_192,
            max_delimiter_depth: 128,
            max_nodes_per_file: 50_000,
            max_nodes: 500_000,
            max_candidates: 1_000,
            max_comparisons: 250_000,
            max_comparison_entries: 20_000_000,
            max_pairs: 5_000,
            max_sites: 10_000,
            workers: 1,
            max_report_bytes: 8 * 1024 * 1024,
            max_name_bytes: 256,
            max_problem_bytes: 2_048,
            max_excerpt_bytes: 512,
        }
    }
}

impl AnalysisLimits {
    /// Configuration cannot raise these parser, memory, or work ceilings.
    pub fn hard_maximum() -> Self {
        Self {
            max_file_bytes: 1024 * 1024,
            max_total_bytes: 128 * 1024 * 1024,
            max_files: 10_000,
            max_discovery_entries: 1_000_000,
            max_raw_units: 16_384,
            max_delimiter_depth: 256,
            max_nodes_per_file: 100_000,
            max_nodes: 2_000_000,
            max_candidates: 5_000,
            max_comparisons: 2_000_000,
            max_comparison_entries: 100_000_000,
            max_pairs: 50_000,
            max_sites: 50_000,
            workers: 4,
            max_report_bytes: 128 * 1024 * 1024,
            max_name_bytes: 1_024,
            max_problem_bytes: 16_384,
            max_excerpt_bytes: 4_096,
        }
    }

    pub fn validate(&self) -> Result<()> {
        let hard = Self::hard_maximum();
        macro_rules! bounded {
            ($($field:ident),+ $(,)?) => {
                $(if self.$field == 0 || self.$field > hard.$field {
                    bail!("{} must be within 1..={}", stringify!($field), hard.$field);
                })+
            };
        }
        bounded!(
            max_file_bytes,
            max_total_bytes,
            max_files,
            max_discovery_entries,
            max_raw_units,
            max_delimiter_depth,
            max_nodes_per_file,
            max_nodes,
            max_candidates,
            max_comparisons,
            max_comparison_entries,
            max_pairs,
            max_sites,
            workers,
            max_report_bytes,
            max_name_bytes,
            max_problem_bytes,
            max_excerpt_bytes,
        );
        if self.max_file_bytes > self.max_total_bytes
            || self.max_files > self.max_discovery_entries
            || self.max_delimiter_depth > self.max_raw_units
            || self.max_nodes_per_file > self.max_nodes
        {
            bail!("per-file limits must fit the corresponding aggregate limits");
        }
        if self.max_report_bytes < MIN_REPORT_BYTES {
            bail!(
                "max_report_bytes must leave at least {MIN_REPORT_BYTES} bytes for an incomplete report"
            );
        }
        Ok(())
    }
}

/// Bound effective configuration without first allocating its JSON encoding.
pub fn validate_configuration<T: Serialize + ?Sized>(configuration: &T) -> Result<()> {
    validate_encoded_bytes(configuration, MAX_CONFIGURATION_BYTES)
}

pub fn validate_report_size<T: Serialize + ?Sized>(
    report: &T,
    limits: &AnalysisLimits,
) -> Result<()> {
    limits.validate()?;
    validate_encoded_bytes(report, limits.max_report_bytes)
}

fn validate_encoded_bytes<T: Serialize + ?Sized>(value: &T, maximum: usize) -> Result<()> {
    encoded_size(value, maximum).map(|_| ())
}

pub(crate) fn encoded_size<T: Serialize + ?Sized>(value: &T, maximum: usize) -> Result<usize> {
    struct Counter {
        bytes: usize,
        maximum: usize,
    }
    impl Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.bytes = self
                .bytes
                .checked_add(bytes.len())
                .ok_or_else(|| io::Error::other("encoded byte count overflow"))?;
            if self.bytes > self.maximum {
                return Err(io::Error::other("encoded JSON exceeds byte limit"));
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter { bytes: 0, maximum };
    serde_json::to_writer(&mut counter, value)?;
    Ok(counter.bytes)
}
