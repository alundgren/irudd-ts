use anyhow::Result;
use archguard::{
    config,
    facts::{Diagnostic, Problem, ProjectFacts},
    plugin, project, rules,
};
use clap::{Parser, Subcommand};
use serde::Serialize;
use std::{path::PathBuf, process::ExitCode, time::Instant};

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
    Check {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long, default_value = "archguard.json")]
        config: PathBuf,
        #[arg(long)]
        json: bool,
    },
    Facts {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long, default_value = "archguard.json")]
        config: PathBuf,
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
    let (root, path, json, facts) = match cli.command {
        Action::Check { root, config, json } => (root, config, json, false),
        Action::Facts { root, config } => (root, config, true, true),
    };
    let (config, cwd) = config::read(&path)?;
    let mut project = project::analyze(&root, &config)?;
    if facts {
        serde_json::to_writer_pretty(std::io::stdout().lock(), &project)?;
        println!();
        return Ok(if project.problems.is_empty() { 0 } else { 2 });
    }
    let mut diagnostics = rules::check(&project, &config)?;
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
