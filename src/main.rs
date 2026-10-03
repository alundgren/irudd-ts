use anyhow::Result;
use archguard::{
    config,
    facts::{Diagnostic, Problem, ProjectFacts},
    plugin, project, rules, semantic,
};
use clap::{Parser, Subcommand};
use serde::Serialize;
use std::{path::PathBuf, process::ExitCode, time::Instant};
mod cli_quality;

#[derive(Parser)]
#[command(
    version,
    about = "Check repository architecture using a shared source module graph"
)]
struct Cli {
    #[command(subcommand)]
    command: Action,
}
#[derive(Subcommand)]
enum Action {
    /// Report structural similarities for reviewer inspection.
    Dryer {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        cache: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Plan controlled TypeScript mutations.
    Mutator {
        #[command(subcommand)]
        command: MutationAction,
    },
    Check {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long, default_value = "archguard.json")]
        config: PathBuf,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        semantic_config: Option<PathBuf>,
        /// Reuse a persistent source graph after validating its inputs.
        #[arg(long)]
        cache: Option<PathBuf>,
    },
    SemanticFacts {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long, default_value = "archguard.json")]
        config: PathBuf,
        #[arg(long)]
        semantic_config: PathBuf,
    },
    Facts {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long, default_value = "archguard.json")]
        config: PathBuf,
        #[arg(long)]
        cache: Option<PathBuf>,
    },
}
#[derive(Subcommand)]
enum MutationAction {
    /// Run explicitly configured trusted tests against isolated mutations.
    Run {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long)]
        config: PathBuf,
        /// Execute a previously inspected plan after revalidating its source.
        #[arg(long)]
        plan: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Enumerate runtime mutation sites without executing commands.
    Plan {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    schema_version: u32,
    files: usize,
    imports: usize,
    complete: bool,
    elapsed_ms: f64,
    diagnostics: Vec<Diagnostic>,
    problems: Vec<Problem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache: Option<archguard::cache::CacheStats>,
}
fn main() -> ExitCode {
    match execute() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("archguard: {error:#}");
            ExitCode::from(2)
        }
    }
}
fn execute() -> Result<u8> {
    let started = Instant::now();
    let cli = Cli::parse();
    let (root, path, json, facts, semantic_path, semantic_only, cache_path) = match cli.command {
        Action::Dryer {
            root,
            config,
            cache,
            json,
        } => {
            return cli_quality::dryer(&root, config.as_deref(), cache.as_deref(), json);
        }
        Action::Mutator {
            command: MutationAction::Plan { root, config, json },
        } => {
            return cli_quality::plan(&root, config.as_deref(), json);
        }
        Action::Mutator {
            command:
                MutationAction::Run {
                    root,
                    config,
                    plan,
                    json,
                },
        } => {
            return cli_quality::run(&root, &config, plan.as_deref(), json);
        }
        Action::Check {
            root,
            config,
            json,
            semantic_config,
            cache,
        } => (root, config, json, false, semantic_config, false, cache),
        Action::Facts {
            root,
            config,
            cache,
        } => (root, config, true, true, None, false, cache),
        Action::SemanticFacts {
            root,
            config,
            semantic_config,
        } => (root, config, true, false, Some(semantic_config), true, None),
    };
    let (config, cwd) = config::read(&path)?;
    let (mut project, cache) = if let Some(path) = cache_path {
        let analysis = project::analyze_cached(&root, &config, &path)?;
        (analysis.project, Some(analysis.cache))
    } else {
        (project::analyze(&root, &config)?, None)
    };
    if facts {
        serde_json::to_writer_pretty(std::io::stdout().lock(), &project)?;
        println!();
        return Ok(if project.problems.is_empty() { 0 } else { 2 });
    }
    let mut semantic_diagnostics = vec![];
    let mut semantic_problems = vec![];
    if let Some(path) = semantic_path {
        let (semantic_config, semantic_cwd) = semantic::SemanticConfig::read(&path)?;
        match semantic::analyze(&project, &semantic_config, &semantic_cwd) {
            Ok(facts) => {
                if semantic_only {
                    serde_json::to_writer_pretty(std::io::stdout().lock(), &facts)?;
                    println!();
                    for problem in &project.problems {
                        eprintln!("{}: incomplete: {}", problem.file, problem.message);
                    }
                    return Ok(if facts.complete && project.problems.is_empty() {
                        0
                    } else {
                        2
                    });
                }
                semantic_diagnostics = semantic::check(&facts, &semantic_config)?;
                semantic_problems.extend(facts.problems().into_iter().map(|message| Problem {
                    file: ".".into(),
                    offset: 0,
                    message,
                }));
            }
            Err(error) => {
                if semantic_only {
                    return Err(error);
                }
                semantic_problems.push(Problem {
                    file: ".".into(),
                    offset: 0,
                    message: format!("semantic provider: {error:#}"),
                });
            }
        }
    }
    let mut diagnostics = rules::check(&project, &config)?;
    diagnostics.extend(semantic_diagnostics);
    for p in &config.plugins {
        match plugin::run(&project, p, &cwd) {
            Ok(ds) => diagnostics.extend(ds),
            Err(e) => project.problems.push(Problem {
                file: ".".into(),
                offset: 0,
                message: format!("{e:#}"),
            }),
        }
    }
    project.problems.extend(semantic_problems);
    diagnostics.sort();
    diagnostics.dedup();
    let report = Report {
        schema_version: 1,
        files: project.files.len(),
        imports: project.files.iter().map(|f| f.imports.len()).sum(),
        complete: project.problems.is_empty(),
        elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
        diagnostics,
        problems: project.problems.clone(),
        cache,
    };
    if json {
        serde_json::to_writer_pretty(std::io::stdout().lock(), &report)?;
        println!();
    } else {
        for d in &report.diagnostics {
            let (line, column) = location(&project, &d.file, d.offset);
            println!("{}:{line}:{column}: {}: {}", d.file, d.rule, d.message);
            if !d.evidence.is_empty() {
                println!("  {}", d.evidence.join(" -> "));
            }
        }
        for p in &report.problems {
            eprintln!("{}: incomplete: {}", p.file, p.message);
        }
        println!(
            "{} files, {} imports, {} violations, {} analysis problems in {:.2} ms",
            report.files,
            report.imports,
            report.diagnostics.len(),
            report.problems.len(),
            report.elapsed_ms
        );
    }
    Ok(if !report.complete {
        2
    } else if report.diagnostics.is_empty() {
        0
    } else {
        1
    })
}
fn location(project: &ProjectFacts, file: &str, offset: usize) -> (usize, usize) {
    std::fs::read_to_string(PathBuf::from(&project.root).join(file))
        .ok()
        .and_then(|source| {
            source.get(..offset).map(|prefix| {
                (
                    prefix.bytes().filter(|b| *b == b'\n').count() + 1,
                    prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1,
                )
            })
        })
        .unwrap_or((1, 1))
}
