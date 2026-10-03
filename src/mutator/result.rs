use super::{
    config::{ExecutionLimits, ReusePolicy},
    facts::{MutationOperator, MutationPlan, MutationSite},
    storage,
};
use crate::quality::{SelectionReport, SourceLocation, validate_sha256};
use anyhow::{Result, bail};
use serde::{Deserialize, Deserializer, Serialize};
use std::{collections::BTreeSet, path::Path};

pub const MAX_REQUEST_BYTES: u64 = 65536;
pub const MAX_RESULT_BYTES: u64 = 8388608;
pub const MAX_IDENTITY_BYTES: usize = 4096;
pub const MAX_MESSAGE_BYTES: usize = 8192;
pub const MAX_FAILURES: usize = 1024;
const MAX_SAFE_INTEGER: u64 = 9007199254740991;

fn nullable<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> std::result::Result<Option<T>, D::Error> {
    Option::<T>::deserialize(deserializer)
}
fn text(value: &str, maximum: usize, empty: bool) -> Result<()> {
    if (!empty && value.is_empty()) || value.contains('\0') || value.len() > maximum {
        bail!("protocol string is empty or exceeds its byte limit");
    }
    Ok(())
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TestExecutionRequest {
    pub schema_version: u32,
    pub request_id: String,
    pub run_id: String,
    pub input_digest: String,
    pub phase: ExecutionPhase,
    #[serde(deserialize_with = "nullable")]
    pub mutation_id: Option<String>,
    pub result_path: std::path::PathBuf,
    pub max_result_bytes: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionPhase {
    Baseline,
    Mutation,
}
impl TestExecutionRequest {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1 {
            bail!("unsupported mutation request version");
        }
        text(&self.request_id, MAX_IDENTITY_BYTES, false)?;
        text(&self.run_id, MAX_IDENTITY_BYTES, false)?;
        validate_sha256(&self.input_digest)?;
        match (&self.phase, &self.mutation_id) {
            (ExecutionPhase::Baseline, None) => {}
            (ExecutionPhase::Mutation, Some(id)) => validate_sha256(id)?,
            _ => bail!("mutation request phase and identity disagree"),
        }
        let path = self
            .result_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("result path must be UTF-8"))?;
        text(path, MAX_IDENTITY_BYTES, false)?;
        if !self.result_path.is_absolute()
            || self.max_result_bytes < 1024
            || self.max_result_bytes > MAX_RESULT_BYTES
        {
            bail!("result path or byte budget unsupported");
        }
        storage::encode(self, MAX_REQUEST_BYTES)?;
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TestExecutionResult {
    pub schema_version: u32,
    pub request_id: String,
    pub run_id: String,
    pub input_digest: String,
    pub complete: bool,
    pub exit_code: i32,
    pub reason: TestCompletionReason,
    pub tests: TestCounts,
    pub failures: Vec<TestFailure>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TestCompletionReason {
    Finished,
    Interrupted,
    InfrastructureError,
}
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TestCounts {
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
}
impl TestCounts {
    pub fn validate(&self) -> Result<()> {
        let total = self
            .passed
            .checked_add(self.failed)
            .and_then(|v| v.checked_add(self.skipped))
            .ok_or_else(|| anyhow::anyhow!("test count overflow"))?;
        if total as u64 > MAX_SAFE_INTEGER {
            bail!("test count exceeds safe integer range");
        }
        Ok(())
    }
    pub fn executed(&self) -> usize {
        self.passed.saturating_add(self.failed)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TestFailure {
    pub kind: TestFailureKind,
    #[serde(deserialize_with = "nullable")]
    pub test_id: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub file: Option<String>,
    pub message: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TestFailureKind {
    Assertion,
    Runtime,
    Import,
    Hook,
    Suite,
    Unhandled,
}
impl TestExecutionResult {
    pub fn read(path: &Path, request: &TestExecutionRequest) -> Result<Self> {
        let result: Self =
            serde_json::from_slice(&storage::read_regular(path, request.max_result_bytes)?)?;
        result.validate(request)?;
        Ok(result)
    }
    pub fn validate(&self, request: &TestExecutionRequest) -> Result<()> {
        request.validate()?;
        if self.schema_version != 1
            || self.request_id != request.request_id
            || self.run_id != request.run_id
            || self.input_digest != request.input_digest
        {
            bail!("mutation response identity disagrees with request");
        }
        if !(0..=255).contains(&self.exit_code)
            || (self.complete && self.reason != TestCompletionReason::Finished)
        {
            bail!("mutation response exit status or completion is inconsistent");
        }
        self.tests.validate()?;
        if self.failures.len() > MAX_FAILURES {
            bail!("too many protocol failure records");
        }
        let mut failed = BTreeSet::new();
        for failure in &self.failures {
            text(&failure.message, MAX_MESSAGE_BYTES, true)?;
            if let Some(id) = &failure.test_id {
                text(id, MAX_IDENTITY_BYTES, false)?;
                failed.insert(id);
            }
            if let Some(file) = &failure.file {
                text(file, MAX_IDENTITY_BYTES, false)?;
            }
            if failure.kind == TestFailureKind::Assertion && failure.test_id.is_none() {
                bail!("assertion failure must name an executed test");
            }
        }
        if self.complete && failed.len() != self.tests.failed {
            bail!("failed test identities disagree with counts");
        }
        if self.complete
            && self.exit_code == 0
            && (self.tests.failed != 0 || !self.failures.is_empty())
        {
            bail!("successful exit disagrees with failures");
        }
        storage::encode(self, request.max_result_bytes)?;
        Ok(())
    }
    pub(crate) fn classify(&self, observed: i32) -> Result<MutationOutcome> {
        if !self.complete
            || self.reason != TestCompletionReason::Finished
            || self.exit_code != observed
            || self.tests.executed() == 0
        {
            bail!("test execution is incomplete or disagrees with observed status");
        }
        if observed == 0 && self.failures.is_empty() && self.tests.failed == 0 {
            return Ok(MutationOutcome::Survived);
        }
        if observed != 0
            && self.tests.failed > 0
            && !self.failures.is_empty()
            && self
                .failures
                .iter()
                .all(|f| f.kind == TestFailureKind::Assertion && f.test_id.is_some())
        {
            return Ok(MutationOutcome::Killed);
        }
        bail!("test command failed outside recognized assertions")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MutationReport {
    pub schema_version: u32,
    pub operator_version: u32,
    pub root: String,
    pub selection: SelectionReport,
    pub input_digest: String,
    pub execution: ExecutionSummary,
    pub complete: bool,
    pub baseline: BaselineResult,
    pub summary: MutationSummary,
    pub results: Vec<MutationResult>,
    pub problems: Vec<ExecutionProblem>,
    pub elapsed_ms: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionSummary {
    pub limits: ExecutionLimits,
    pub workspace: WorkspaceSummary,
    pub reuse: ReusePolicy,
    pub reuse_reason: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceSummary {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub dependency_destinations: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BaselineResult {
    pub outcome: BaselineOutcome,
    pub tests: Option<TestCounts>,
    pub exit_code: Option<i32>,
    pub elapsed_ms: f64,
    pub message: Option<String>,
}
impl Default for BaselineResult {
    fn default() -> Self {
        Self {
            outcome: BaselineOutcome::NotRun,
            tests: None,
            exit_code: None,
            elapsed_ms: 0.0,
            message: None,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BaselineOutcome {
    Passed,
    Failed,
    ExecutionError,
    TimedOut,
    Cancelled,
    NotRun,
}
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MutationSummary {
    pub planned: usize,
    pub executed: usize,
    pub reused: usize,
    pub killed: usize,
    pub survived: usize,
    pub invalid_mutants: usize,
    pub execution_errors: usize,
    pub timed_out: usize,
    pub cancelled: usize,
    pub not_run: usize,
    pub omitted_results: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MutationResult {
    pub mutation_id: String,
    pub location: SourceLocation,
    pub operator: MutationOperator,
    pub expected: String,
    pub replacement: String,
    pub context: Option<SourceContext>,
    pub outcome: MutationOutcome,
    pub reused: bool,
    pub tests: Option<TestCounts>,
    pub exit_code: Option<i32>,
    pub elapsed_ms: f64,
    pub message: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceContext {
    pub before: String,
    pub after: String,
    pub truncated_before: bool,
    pub truncated_after: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MutationOutcome {
    Killed,
    Survived,
    InvalidMutant,
    ExecutionError,
    TimedOut,
    Cancelled,
    NotRun,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionProblem {
    pub kind: ExecutionProblemKind,
    pub mutation_id: Option<String>,
    pub file: Option<String>,
    pub message: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionProblemKind {
    InvalidPlan,
    InputChanged,
    Workspace,
    Baseline,
    Command,
    Protocol,
    Limit,
    Cancellation,
    Cleanup,
    State,
}

pub(crate) fn bounded_message(message: &str) -> String {
    clip(message, MAX_MESSAGE_BYTES).0.to_owned()
}
fn clip(text: &str, maximum: usize) -> (&str, bool) {
    let mut end = text.len().min(maximum);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (&text[..end], end < text.len())
}
impl MutationResult {
    pub(crate) fn initial(site: &MutationSite) -> Self {
        Self {
            mutation_id: site.id.clone(),
            location: site.location.clone(),
            operator: site.operator,
            expected: site.expected.clone(),
            replacement: site.replacement.clone(),
            context: None,
            outcome: MutationOutcome::NotRun,
            reused: false,
            tests: None,
            exit_code: None,
            elapsed_ms: 0.0,
            message: None,
        }
    }
    pub(crate) fn source_context(&mut self, source: &str) {
        if self.location.end > source.len()
            || !source.is_char_boundary(self.location.start)
            || !source.is_char_boundary(self.location.end)
        {
            return;
        }
        let prefix = &source[..self.location.start];
        let suffix = &source[self.location.end..];
        let prefix = &prefix[prefix.rfind('\n').map_or(0, |at| at + 1)..];
        let suffix = &suffix[..suffix.find('\n').unwrap_or(suffix.len())];
        let mut start = prefix.len().saturating_sub(256);
        while !prefix.is_char_boundary(start) {
            start += 1;
        }
        let (after, truncated_after) = clip(suffix, 256);
        self.context = Some(SourceContext {
            before: prefix[start..].into(),
            after: after.into(),
            truncated_before: start != 0,
            truncated_after,
        });
    }
}
pub(crate) struct EvidenceBudget {
    used: u64,
    problem_count: usize,
    omitted: Vec<(usize, MutationOutcome, bool)>,
}
impl EvidenceBudget {
    pub fn new(report: &MutationReport) -> Self {
        Self {
            used: storage::encoded_size(report, report.execution.limits.max_report_bytes)
                .unwrap_or(report.execution.limits.max_report_bytes),
            problem_count: report.problems.len(),
            omitted: vec![],
        }
    }
}
impl MutationSummary {
    pub(crate) fn add_outcome(&mut self, outcome: MutationOutcome, reused: bool) {
        if reused {
            self.reused += 1;
        } else if !matches!(
            outcome,
            MutationOutcome::InvalidMutant | MutationOutcome::NotRun
        ) {
            self.executed += 1;
        }
        match outcome {
            MutationOutcome::Killed => self.killed += 1,
            MutationOutcome::Survived => self.survived += 1,
            MutationOutcome::InvalidMutant => self.invalid_mutants += 1,
            MutationOutcome::ExecutionError => self.execution_errors += 1,
            MutationOutcome::TimedOut => self.timed_out += 1,
            MutationOutcome::Cancelled => self.cancelled += 1,
            MutationOutcome::NotRun => self.not_run += 1,
        }
    }
}
impl MutationReport {
    pub(crate) fn initial(plan: &MutationPlan, execution: ExecutionSummary) -> Self {
        let mut report = Self {
            schema_version: 1,
            operator_version: plan.operator_version,
            root: plan.root.clone(),
            selection: SelectionReport {
                requested: plan.selection.requested.clone(),
                selected: vec![],
                skipped: vec![],
                complete_within_selection: false,
            },
            input_digest: String::new(),
            execution,
            complete: false,
            baseline: BaselineResult::default(),
            summary: MutationSummary {
                planned: plan.sites.len(),
                ..MutationSummary::default()
            },
            results: vec![],
            problems: vec![],
            elapsed_ms: 0.0,
        };
        let maximum = report
            .execution
            .limits
            .max_report_bytes
            .saturating_sub(16384);
        let mut used = storage::encoded_size(&report, maximum).unwrap_or(maximum);
        let selection_size = storage::encoded_size(&plan.selection, maximum);
        let empty_selection = storage::encoded_size(&report.selection, maximum).unwrap_or(0);
        if let Ok(bytes) = selection_size
            && used.saturating_sub(empty_selection).saturating_add(bytes) <= maximum
        {
            used = used - empty_selection + bytes;
            report.selection = plan.selection.clone();
        } else {
            report.summary.omitted_results = plan.sites.len();
            report.problem(
                ExecutionProblemKind::Limit,
                None,
                None,
                "source evidence exceeds report budget; selected/skipped source records omitted",
            );
            return report;
        }
        if plan.sites.len() > report.execution.limits.max_mutants {
            report.summary.omitted_results = plan.sites.len();
            report.problem(
                ExecutionProblemKind::Limit,
                None,
                None,
                "mutation count exceeds configured execution limit; result records omitted",
            );
            return report;
        }
        for site in &plan.sites {
            let row = MutationResult::initial(site);
            let bytes = storage::encoded_size(&row, maximum)
                .unwrap_or(maximum)
                .saturating_add(1);
            if used.saturating_add(bytes) > maximum {
                report.summary.omitted_results = plan.sites.len() - report.results.len();
                report.problem(ExecutionProblemKind::Limit,None,None,"planned mutation metadata exceeds report budget; remaining result records omitted");
                return report;
            }
            used += bytes;
            report.results.push(row);
        }
        report
    }
    pub(crate) fn install_result(
        &mut self,
        index: usize,
        mut result: MutationResult,
        budget: &mut EvidenceBudget,
    ) -> bool {
        if budget.problem_count != self.problems.len() {
            budget.used = storage::encoded_size(self, self.execution.limits.max_report_bytes)
                .unwrap_or(self.execution.limits.max_report_bytes);
            budget.problem_count = self.problems.len();
        }
        let old =
            storage::encoded_size(&self.results[index], self.execution.limits.max_report_bytes)
                .unwrap_or(0);
        let new = storage::encoded_size(&result, self.execution.limits.max_report_bytes)
            .unwrap_or(self.execution.limits.max_report_bytes);
        let fits = budget.used.saturating_sub(old).saturating_add(new)
            <= self.execution.limits.max_report_bytes.saturating_sub(16384);
        if !fits {
            budget.omitted.push((index, result.outcome, result.reused));
            result.context = None;
            result.message = None;
            result.tests = None;
            result.outcome = MutationOutcome::ExecutionError;
            self.summary.omitted_results += 1;
        }
        let retained =
            storage::encoded_size(&result, self.execution.limits.max_report_bytes).unwrap_or(new);
        budget.used = budget.used.saturating_sub(old).saturating_add(retained);
        self.results[index] = result;
        fits
    }
    pub(crate) fn omit_results(&mut self, budget: &EvidenceBudget) {
        let mut indexes: Vec<_> = budget.omitted.iter().map(|(index, _, _)| *index).collect();
        indexes.sort_unstable();
        indexes.dedup();
        for index in indexes.into_iter().rev() {
            self.results.remove(index);
        }
    }
    pub(crate) fn count_omitted(&mut self, budget: &EvidenceBudget) {
        for (_, outcome, reused) in &budget.omitted {
            self.summary.add_outcome(*outcome, *reused);
        }
    }
    pub(crate) fn problem(
        &mut self,
        kind: ExecutionProblemKind,
        id: Option<String>,
        file: Option<String>,
        message: &str,
    ) {
        if self.problems.len() >= 16 {
            return;
        }
        self.problems.push(ExecutionProblem {
            kind,
            mutation_id: id,
            file,
            message: bounded_message(message),
        });
        if storage::encoded_size(self, self.execution.limits.max_report_bytes).is_err() {
            self.problems.pop();
            self.complete = false;
            self.problems.clear();
            self.problems.push(ExecutionProblem {
                kind: ExecutionProblemKind::Limit,
                mutation_id: None,
                file: None,
                message: "execution problems omitted because report byte budget was exhausted"
                    .into(),
            });
        }
    }
    pub(crate) fn finish(&mut self, elapsed: f64) -> Result<()> {
        self.elapsed_ms = elapsed;
        self.results.sort_by(|a, b| {
            (&a.location.file, a.location.start, &a.mutation_id).cmp(&(
                &b.location.file,
                b.location.start,
                &b.mutation_id,
            ))
        });
        let planned = self.summary.planned;
        let omitted = self.summary.omitted_results;
        self.summary = MutationSummary {
            planned,
            omitted_results: omitted,
            ..MutationSummary::default()
        };
        for result in &self.results {
            if result.reused {
                self.summary.reused += 1;
            } else if !matches!(
                result.outcome,
                MutationOutcome::InvalidMutant | MutationOutcome::NotRun
            ) {
                self.summary.executed += 1;
            }
            match result.outcome {
                MutationOutcome::Killed => self.summary.killed += 1,
                MutationOutcome::Survived => self.summary.survived += 1,
                MutationOutcome::InvalidMutant => self.summary.invalid_mutants += 1,
                MutationOutcome::ExecutionError => self.summary.execution_errors += 1,
                MutationOutcome::TimedOut => self.summary.timed_out += 1,
                MutationOutcome::Cancelled => self.summary.cancelled += 1,
                MutationOutcome::NotRun => self.summary.not_run += 1,
            }
        }
        self.complete = self.baseline.outcome == BaselineOutcome::Passed
            && self.problems.is_empty()
            && self.summary.omitted_results == 0
            && self.results.len() == planned
            && self.summary.execution_errors
                + self.summary.timed_out
                + self.summary.cancelled
                + self.summary.not_run
                == 0;
        if storage::encode(self, self.execution.limits.max_report_bytes).is_err() {
            self.complete = false;
            let original = self.results.len();
            self.results.clear();
            self.summary.omitted_results += original;
            self.problem(
                ExecutionProblemKind::Limit,
                None,
                None,
                "report byte budget exhausted; result/source evidence omitted and selection marked incomplete",
            );
            self.problems.truncate(16);
            while storage::encode(self, self.execution.limits.max_report_bytes).is_err() {
                if !self.selection.skipped.is_empty() {
                    self.selection
                        .skipped
                        .truncate(self.selection.skipped.len() / 2);
                } else if !self.selection.selected.is_empty() {
                    self.selection
                        .selected
                        .truncate(self.selection.selected.len() / 2);
                } else if self.problems.len() > 1 {
                    self.problems.truncate(self.problems.len() / 2);
                } else {
                    return Err(anyhow::anyhow!(
                        "effective configuration cannot fit the report budget"
                    ));
                }
                self.selection.complete_within_selection = false;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> TestExecutionRequest {
        TestExecutionRequest {
            schema_version: 1,
            request_id: "task-1".into(),
            run_id: "run-1".into(),
            input_digest: "a".repeat(64),
            phase: ExecutionPhase::Mutation,
            mutation_id: Some("b".repeat(64)),
            result_path: "/tmp/result.json".into(),
            max_result_bytes: 1048576,
        }
    }
    fn killed() -> TestExecutionResult {
        let request = request();
        TestExecutionResult {
            schema_version: 1,
            request_id: request.request_id,
            run_id: request.run_id,
            input_digest: request.input_digest,
            complete: true,
            exit_code: 1,
            reason: TestCompletionReason::Finished,
            tests: TestCounts {
                passed: 1,
                failed: 1,
                skipped: 0,
            },
            failures: vec![TestFailure {
                kind: TestFailureKind::Assertion,
                test_id: Some("test-1".into()),
                file: None,
                message: "expected 1 to equal 0".into(),
            }],
        }
    }
    #[test]
    fn only_matching_complete_assertions_kill() {
        let mut result = killed();
        result.validate(&request()).unwrap();
        assert_eq!(result.classify(1).unwrap(), MutationOutcome::Killed);
        assert!(result.classify(0).is_err());
        result.failures.push(TestFailure {
            kind: TestFailureKind::Hook,
            test_id: None,
            file: None,
            message: "late teardown".into(),
        });
        result.validate(&request()).unwrap();
        assert!(result.classify(1).is_err());
        result.failures.pop();
        result.complete = false;
        assert!(result.classify(1).is_err());
        result.complete = true;
        result.tests.failed = 2;
        assert!(result.validate(&request()).is_err());
        result.tests.failed = 1;
        result.request_id = "stale".into();
        assert!(result.validate(&request()).is_err());
        result = killed();
        result.tests = TestCounts {
            passed: 1,
            failed: 0,
            skipped: 0,
        };
        result.failures.clear();
        result.exit_code = 0;
        result.validate(&request()).unwrap();
        assert_eq!(result.classify(0).unwrap(), MutationOutcome::Survived);
        result.tests.passed = 0;
        result.validate(&request()).unwrap();
        assert!(result.classify(0).is_err());
    }
    #[test]
    fn strict_wire_nullable_fields_exit_code_and_budgets() {
        let mut value = serde_json::to_value(killed()).unwrap();
        value.as_object_mut().unwrap().remove("exitCode");
        assert!(serde_json::from_value::<TestExecutionResult>(value.clone()).is_err());
        value["exitCode"] = serde_json::Value::Null;
        assert!(serde_json::from_value::<TestExecutionResult>(value).is_err());
        let mut value = serde_json::to_value(killed()).unwrap();
        value["failures"][0].as_object_mut().unwrap().remove("file");
        assert!(serde_json::from_value::<TestExecutionResult>(value).is_err());
        let mut result = killed();
        result.exit_code = 256;
        assert!(result.validate(&request()).is_err());
        result.exit_code = 1;
        result.failures[0].message = "x".repeat(8193);
        assert!(result.validate(&request()).is_err());
        result.failures[0].message = "x".repeat(8192);
        result.validate(&request()).unwrap();
        let mut request = request();
        request.max_result_bytes = 1024;
        assert!(result.validate(&request).is_err());
        assert!(
            serde_json::from_str::<TestCounts>(r#"{"passed":1,"passed":2,"failed":0,"skipped":0}"#)
                .is_err()
        );
    }
    #[test]
    fn context_preserves_unicode_crlf_and_exact_operators() {
        let source = format!("{} < {}\r\n", "λ".repeat(200), "界".repeat(120));
        let start = source.find('<').unwrap();
        let site = MutationSite {
            id: "a".repeat(64),
            location: SourceLocation {
                file: "x.ts".into(),
                start,
                end: start + 1,
                line: 1,
                end_line: 1,
            },
            owner: None,
            operator: MutationOperator::Comparison,
            expected: "<".into(),
            replacement: "<=".into(),
            source_sha256: storage::digest(source.as_bytes()),
        };
        let mut result = MutationResult::initial(&site);
        result.source_context(&source);
        let context = result.context.unwrap();
        assert!(context.before.len() <= 256 && context.after.len() <= 256);
        assert!(context.truncated_before && context.truncated_after);
        assert!(context.before.ends_with(' '));
        assert!(context.after.starts_with(' '));
        let mut result = MutationResult::initial(&site);
        result.location.start = 1;
        result.source_context(&source);
        assert!(result.context.is_none());
    }
}

#[cfg(test)]
mod evidence_tests {
    use super::*;
    #[test]
    fn report_count_and_aggregate_messages_are_reserved_before_retention() {
        let root =
            storage::create_private_directory(&std::env::temp_dir(), "archguard-evidence-test")
                .unwrap();
        let source = (0..50)
            .map(|index| format!("export const flag{index} = true;\n"))
            .collect::<String>();
        std::fs::write(root.join("a.ts"), source).unwrap();
        let configuration = super::super::facts::MutationPlanConfig {
            operators: vec![MutationOperator::Boolean],
            ..Default::default()
        };
        let plan = super::super::plan(&root, &configuration).unwrap();
        assert!(plan.complete);
        assert_eq!(plan.sites.len(), 50);
        let limits = ExecutionLimits {
            max_report_bytes: 65536,
            ..Default::default()
        };
        let execution = ExecutionSummary {
            limits: limits.clone(),
            workspace: WorkspaceSummary {
                include: vec!["**".into()],
                exclude: vec![],
                dependency_destinations: vec![],
            },
            reuse: ReusePolicy::Off,
            reuse_reason: None,
        };
        let mut report = MutationReport::initial(&plan, execution.clone());
        assert!(report.problems.is_empty());
        report.baseline.outcome = BaselineOutcome::Passed;
        report.baseline.tests = Some(TestCounts {
            passed: 1,
            failed: 0,
            skipped: 0,
        });
        let mut budget = EvidenceBudget::new(&report);
        let mut rejected = false;
        for (index, site) in plan.sites.iter().enumerate() {
            let mut result = MutationResult::initial(site);
            result.outcome = MutationOutcome::Killed;
            result.tests = Some(TestCounts {
                passed: 0,
                failed: 1,
                skipped: 0,
            });
            result.exit_code = Some(1);
            result.message = Some("assertion context ".repeat(400));
            if !report.install_result(index, result, &mut budget) {
                rejected = true;
                report.problem(
                    ExecutionProblemKind::Limit,
                    None,
                    None,
                    "result evidence omitted",
                );
                break;
            }
        }
        assert!(rejected);
        assert!(
            report
                .results
                .iter()
                .filter_map(|result| result.message.as_ref())
                .map(String::len)
                .sum::<usize>()
                < 65536
        );
        report.omit_results(&budget);
        report.finish(0.0).unwrap();
        report.count_omitted(&budget);
        assert!(!report.complete);
        assert_eq!(report.summary.omitted_results, 1);
        assert_eq!(report.results.len() + report.summary.omitted_results, 50);
        assert!(storage::encoded_size(&report, 65536).is_ok());
        let mut execution = execution;
        execution.limits.max_mutants = 1;
        let limited = MutationReport::initial(&plan, execution);
        assert!(limited.results.is_empty());
        assert_eq!(limited.summary.omitted_results, 50);
        assert!(
            limited
                .problems
                .iter()
                .any(|problem| problem.kind == ExecutionProblemKind::Limit)
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
