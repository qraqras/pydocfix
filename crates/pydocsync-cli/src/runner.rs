use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use rayon::ThreadPoolBuilder;
use rayon::prelude::*;

use crate::args::{parse_cli_args, print_help};
use crate::config::ProjectConfig;
use crate::files::collect_python_files;
use crate::fixer::{apply_fixes_until_stable, render_diff};
use crate::render::DiagnosticRenderer;
use crate::settings::ResolvedSettings;

pub(crate) fn run() -> Result<(), String> {
    let Some(cli_args) = parse_cli_args(std::env::args().skip(1))? else {
        print_help();
        return Ok(());
    };

    let project_config = ProjectConfig::load(None, &cli_args.paths)?;
    let settings = ResolvedSettings::resolve(cli_args, project_config);
    let paths = collect_python_files(
        settings.paths.clone(),
        &settings.exclude,
        settings.project_root.as_deref(),
    )?;
    let stdout = io::stdout();
    let mut output = stdout.lock();
    let mut exit_violation_count = 0usize;
    let mut file_outcomes = process_files(&paths, &settings)?;
    file_outcomes.sort_by(|left, right| left.path.cmp(&right.path));

    for file_outcome in file_outcomes {
        for rendered_line in file_outcome.rendered_lines {
            write_line(&mut output, format_args!("{rendered_line}"))?;
        }
        if let Some(fixed_source) = file_outcome.fixed_source {
            fs::write(&file_outcome.path, fixed_source)
                .map_err(|error| format!("{}: {error}", file_outcome.path.display()))?;
        }
        exit_violation_count += file_outcome.exit_violation_count;
    }

    if exit_violation_count > 0 {
        std::process::exit(1);
    }

    Ok(())
}

#[derive(Debug)]
struct FileOutcome {
    path: PathBuf,
    rendered_lines: Vec<String>,
    fixed_source: Option<String>,
    exit_violation_count: usize,
}

fn process_files(paths: &[PathBuf], settings: &ResolvedSettings) -> Result<Vec<FileOutcome>, String> {
    let run = || {
        paths
            .par_iter()
            .map(|path| process_file(path, settings))
            .collect::<Vec<_>>()
    };
    let outcomes = if let Some(jobs) = settings.jobs {
        ThreadPoolBuilder::new()
            .num_threads(jobs)
            .build()
            .map_err(|error| format!("failed to initialize worker pool: {error}"))?
            .install(run)
    } else {
        run()
    };

    outcomes.into_iter().collect()
}

fn process_file(path: &Path, settings: &ResolvedSettings) -> Result<FileOutcome, String> {
    let source = fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let linter = settings.linter();
    let report = linter.analyze_source(&source);
    let diagnostics = settings.filter_diagnostics(report.diagnostics);
    if settings.fix || settings.diff {
        let outcome = apply_fixes_until_stable(&source, settings.unsafe_fixes, |current_source| {
            let current_report = linter.analyze_source(current_source);
            settings.filter_diagnostics(current_report.diagnostics)
        })?;
        let mut rendered_lines = Vec::new();
        if settings.diff
            && let Some(rendered_diff) = render_diff(path, &source, &outcome.source)
        {
            rendered_lines.push(rendered_diff);
        }
        if !settings.diff {
            let renderer = DiagnosticRenderer::new(path, &source);
            rendered_lines.extend(
                outcome
                    .remaining_diagnostics
                    .iter()
                    .map(|diagnostic| renderer.render(diagnostic)),
            );
        }
        let fixed_source = (settings.fix && outcome.source != source).then_some(outcome.source);
        let exit_violation_count = if settings.fix {
            outcome.remaining_diagnostics.len()
        } else {
            diagnostics.len()
        };
        return Ok(FileOutcome {
            path: path.to_path_buf(),
            rendered_lines,
            fixed_source,
            exit_violation_count,
        });
    }

    let renderer = DiagnosticRenderer::new(path, &source);
    let rendered_lines = diagnostics
        .iter()
        .map(|diagnostic| renderer.render(diagnostic))
        .collect();
    Ok(FileOutcome {
        path: path.to_path_buf(),
        rendered_lines,
        fixed_source: None,
        exit_violation_count: diagnostics.len(),
    })
}

fn write_line(output: &mut impl Write, args: std::fmt::Arguments<'_>) -> Result<(), String> {
    if let Err(error) = writeln!(output, "{args}") {
        if error.kind() == io::ErrorKind::BrokenPipe {
            return Ok(());
        }
        return Err(format!("failed to write output: {error}"));
    }
    Ok(())
}
