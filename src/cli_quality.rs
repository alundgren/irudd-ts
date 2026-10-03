use anyhow::{Context, Result, ensure};
use archguard::{
    dryer::{self, DryerConfig},
    mutator::{self, MutationPlanConfig},
    quality::MAX_CONFIGURATION_BYTES,
};
use serde::{Serialize, de::DeserializeOwned};
use std::{fs::OpenOptions, io::Read, path::Path};

pub(super) fn dryer(
    root: &Path,
    configuration: Option<&Path>,
    cache: Option<&Path>,
    json: bool,
) -> Result<u8> {
    let configuration: DryerConfig = configuration
        .map(|path| read_json(path, MAX_CONFIGURATION_BYTES))
        .transpose()?
        .unwrap_or_default();
    configuration.validate()?;
    let report = match cache {
        Some(cache) => dryer::analyze_cached(root, &configuration, cache)?,
        None => dryer::analyze(root, &configuration)?,
    };
    if json {
        print_json(&report)?;
    } else {
        println!(
            "dryer: {} selected files, {} candidate functions, {} similar pairs; {}",
            report.files.len(),
            report.functions.len(),
            report.pairs.len(),
            completion(report.complete),
        );
        for pair in &report.pairs {
            println!(
                "{}:{} {} <> {}:{} {}  set {:.3}, multiset {:.3}, weighted {:.3}",
                display(&pair.left.location.file),
                pair.left.location.line,
                display(&pair.left.name),
                display(&pair.right.location.file),
                pair.right.location.line,
                display(&pair.right.name),
                pair.similarity.set,
                pair.similarity.multiset,
                pair.similarity.weighted,
            );
            if pair.left_opaque_nodes + pair.right_opaque_nodes > 0 {
                println!("  Contains syntax compared as exact source; inspect the pair.");
            }
        }
        print_analysis_problems(&report.problems);
        if !report.omitted_evidence.is_empty() {
            println!("Some evidence exceeded configured limits; use JSON for omission counts.");
        }
        println!(
            "Similar structure is reviewer evidence. Inspect whether the duplication is intended."
        );
    }
    Ok(if report.complete { 0 } else { 2 })
}

pub(super) fn plan(root: &Path, configuration: Option<&Path>, json: bool) -> Result<u8> {
    let configuration: MutationPlanConfig = configuration
        .map(|path| read_json(path, MAX_CONFIGURATION_BYTES))
        .transpose()?
        .unwrap_or_default();
    configuration.validate()?;
    let report = mutator::plan(root, &configuration)?;
    if json {
        print_json(&report)?;
    } else {
        println!(
            "mutator plan: {} selected files, {} mutation sites; {}",
            report.files.len(),
            report.sites.len(),
            completion(report.complete),
        );
        for site in &report.sites {
            println!(
                "{}:{} bytes {}..{}  {:?}: {} -> {}  {}",
                display(&site.location.file),
                site.location.line,
                site.location.start,
                site.location.end,
                site.operator,
                display(&site.expected),
                display(&site.replacement),
                site.id,
            );
        }
        print_analysis_problems(&report.problems);
        if !report.omitted_evidence.is_empty() {
            println!("Some evidence exceeded configured limits; use JSON for omission counts.");
        }
    }
    Ok(if report.complete { 0 } else { 2 })
}

pub(super) fn run(
    root: &Path,
    configuration: &Path,
    plan_path: Option<&Path>,
    json: bool,
) -> Result<u8> {
    let (configuration, directory) = mutator::MutatorConfig::read(configuration)?;
    let plan = match plan_path {
        Some(path) => {
            let plan: mutator::MutationPlan =
                read_json(path, configuration.plan.limits.max_report_bytes)?;
            plan.validate()?;
            ensure!(
                plan.configuration == configuration.plan,
                "stored plan configuration disagrees with run configuration"
            );
            ensure!(
                Path::new(&plan.root).canonicalize()? == root.canonicalize()?,
                "stored plan root disagrees with selected root"
            );
            plan
        }
        None => mutator::plan(root, &configuration.plan)?,
    };
    let cancellation = CliCancellation::install()?;
    let report = mutator::run(
        &plan,
        &configuration.execution,
        &directory,
        &cancellation.token,
    )?;
    if json {
        print_json(&report)?;
    } else {
        println!(
            "mutator: baseline {:?}; {}",
            report.baseline.outcome,
            completion(report.complete)
        );
        println!(
            "{} planned, {} executed, {} reused, {} killed, {} survived",
            report.summary.planned,
            report.summary.executed,
            report.summary.reused,
            report.summary.killed,
            report.summary.survived
        );
        for result in &report.results {
            if matches!(result.outcome, mutator::MutationOutcome::Killed) {
                continue;
            }
            println!(
                "{:?} {}:{} bytes {}..{}  {} -> {}{}",
                result.outcome,
                display(&result.location.file),
                result.location.line,
                result.location.start,
                result.location.end,
                display(&result.expected),
                display(&result.replacement),
                if result.reused { " [reused]" } else { "" }
            );
            if let Some(context) = &result.context {
                println!(
                    "  original: {}{}{}",
                    display(&context.before),
                    display(&result.expected),
                    display(&context.after)
                );
                println!(
                    "  mutant:   {}{}{}",
                    display(&context.before),
                    display(&result.replacement),
                    display(&context.after)
                );
                if context.truncated_before || context.truncated_after {
                    println!(
                        "  Source context is truncated; use byte offsets to inspect the file."
                    );
                }
            }
            if let Some(message) = &result.message {
                println!("  {}", display(message));
            }
        }
        for problem in &report.problems {
            println!("{:?}: {}", problem.kind, display(&problem.message));
        }
        if report.summary.omitted_results != 0 {
            println!(
                "{} results exceeded the report limit.",
                report.summary.omitted_results
            );
        }
        println!(
            "Inspect survivors against the specification. Equivalent mutations can survive correct tests."
        );
    }
    Ok(if report.complete { 0 } else { 2 })
}

#[cfg(unix)]
static CANCELLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(unix)]
extern "C" fn cancel_run(_: nix::libc::c_int) {
    CANCELLED.store(true, std::sync::atomic::Ordering::Relaxed);
}
struct CliCancellation {
    token: mutator::CancellationToken,
    #[cfg(unix)]
    previous: Vec<(nix::sys::signal::Signal, nix::sys::signal::SigAction)>,
}
impl CliCancellation {
    fn install() -> Result<Self> {
        #[cfg(unix)]
        {
            use nix::sys::signal::{self, SaFlags, SigAction, SigHandler, SigSet, Signal};
            CANCELLED.store(false, std::sync::atomic::Ordering::Relaxed);
            let mut guard = Self {
                token: mutator::CancellationToken::from_static_flag(&CANCELLED),
                previous: vec![],
            };
            let action = SigAction::new(
                SigHandler::Handler(cancel_run),
                SaFlags::empty(),
                SigSet::empty(),
            );
            for signal in [Signal::SIGINT, Signal::SIGTERM] {
                // The handler only stores to a static atomic. The CLI owns signal
                // setup; library callers keep control of their own handlers.
                let previous = unsafe { signal::sigaction(signal, &action) }?;
                guard.previous.push((signal, previous));
            }
            Ok(guard)
        }
        #[cfg(not(unix))]
        {
            Ok(Self {
                token: mutator::CancellationToken::new(),
            })
        }
    }
}
impl Drop for CliCancellation {
    fn drop(&mut self) {
        #[cfg(unix)]
        for (signal, action) in self.previous.iter().rev() {
            // Restore the process handlers installed before this CLI command.
            let _ = unsafe { nix::sys::signal::sigaction(*signal, action) };
        }
    }
}

fn print_analysis_problems(problems: &[archguard::quality::AnalysisProblem]) {
    for problem in problems {
        println!(
            "{}:byte {}: {:?}: {}{}",
            display(&problem.file),
            problem.offset,
            problem.kind,
            display(&problem.message),
            if problem.message_truncated {
                " [truncated]"
            } else {
                ""
            },
        );
    }
}

fn completion(complete: bool) -> &'static str {
    if complete {
        "complete within selection"
    } else {
        "incomplete"
    }
}

fn display(text: &str) -> String {
    text.escape_debug().to_string()
}

pub(super) fn print_json<T: Serialize>(value: &T) -> Result<()> {
    // Compact output respects the library's aggregate encoded-byte budget.
    serde_json::to_writer(std::io::stdout().lock(), value)?;
    Ok(())
}

pub(super) fn read_json<T: DeserializeOwned>(path: &Path, maximum: usize) -> Result<T> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .with_context(|| format!("read {}", path.display()))?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() <= maximum as u64,
        "configuration must be a bounded regular file"
    );
    let mut bytes = vec![];
    file.take(maximum as u64 + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= maximum, "JSON file exceeds byte limit");
    serde_json::from_slice(&bytes).with_context(|| format!("invalid JSON in {}", path.display()))
}
