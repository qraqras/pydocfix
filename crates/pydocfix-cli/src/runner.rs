use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use rayon::ThreadPoolBuilder;
use rayon::prelude::*;

use crate::args::{parse_cli_args, print_help};
use crate::baseline::{
    BaselineEntry, compute_updated_baseline, filter_baseline_violations, generate_baseline, load_baseline,
    normalize_path, write_baseline,
};
use crate::config::ProjectConfig;
use crate::files::collect_python_files;
use crate::fixer::{apply_fixes_until_stable, render_diff};
use crate::render::DiagnosticRenderer;
use crate::settings::ResolvedSettings;
use crate::suppression::apply_noqa_suppression_with_report;

pub(crate) fn run() -> Result<(), String> {
    let Some(cli_args) = parse_cli_args(std::env::args().skip(1))? else {
        print_help();
        return Ok(());
    };

    let project_config = ProjectConfig::load(cli_args.config_path.as_deref(), &cli_args.paths)?;
    let settings = ResolvedSettings::resolve(cli_args, project_config);
    let baseline_data = if let Some(path) = settings.baseline_path.as_deref()
        && !settings.generate_baseline
    {
        load_baseline(path)?
    } else {
        BTreeMap::new()
    };
    let paths = collect_python_files(
        settings.paths.clone(),
        &settings.exclude,
        settings.project_root.as_deref(),
    )?;
    let stdout = io::stdout();
    let mut output = stdout.lock();
    let mut exit_violation_count = 0usize;
    let mut raw_violations_by_file = BTreeMap::new();
    let mut file_outcomes = process_files(&paths, &settings, &baseline_data)?;
    file_outcomes.sort_by(|left, right| left.path.cmp(&right.path));

    for file_outcome in file_outcomes {
        if !file_outcome.raw_diagnostics.is_empty() {
            raw_violations_by_file.insert(file_outcome.baseline_key.clone(), file_outcome.raw_diagnostics);
        }
        for rendered_line in file_outcome.rendered_lines {
            write_line(&mut output, format_args!("{rendered_line}"))?;
        }
        if let Some(fixed_source) = file_outcome.fixed_source {
            fs::write(&file_outcome.path, fixed_source)
                .map_err(|error| format!("{}: {error}", file_outcome.path.display()))?;
        }
        exit_violation_count += file_outcome.exit_violation_count;
    }

    if settings.generate_baseline {
        let Some(path) = settings.baseline_path.as_deref() else {
            return Err("specify a baseline path with --baseline or [tool.pydocfix] baseline".to_string());
        };
        let baseline = generate_baseline(&raw_violations_by_file);
        write_baseline(&baseline, path)?;
        write_line(&mut output, format_args!("Baseline generated: {}", path.display()))?;
        return Ok(());
    }

    if !baseline_data.is_empty()
        && let Some(path) = settings.baseline_path.as_deref()
    {
        let (changed, updated) = compute_updated_baseline(&baseline_data, &raw_violations_by_file);
        if changed {
            write_baseline(&updated, path)?;
        }
    }

    if exit_violation_count > 0 {
        std::process::exit(1);
    }

    Ok(())
}

#[derive(Debug)]
struct FileOutcome {
    path: PathBuf,
    baseline_key: String,
    raw_diagnostics: Vec<pydocfix_core::Diagnostic>,
    rendered_lines: Vec<String>,
    fixed_source: Option<String>,
    exit_violation_count: usize,
}

fn process_files(
    paths: &[PathBuf],
    settings: &ResolvedSettings,
    baseline_data: &BTreeMap<String, Vec<BaselineEntry>>,
) -> Result<Vec<FileOutcome>, String> {
    let run = || {
        paths
            .par_iter()
            .map(|path| process_file(path, settings, baseline_data))
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

fn process_file(
    path: &Path,
    settings: &ResolvedSettings,
    baseline_data: &BTreeMap<String, Vec<BaselineEntry>>,
) -> Result<FileOutcome, String> {
    let baseline_key = normalize_path(path, settings.project_root.as_deref().unwrap_or_else(|| Path::new(".")));
    let source = fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let linter = settings.linter();
    let report = linter.analyze_source(&source);
    let record_raw_diagnostics = settings.generate_baseline || !baseline_data.is_empty();
    if settings.debug_docstrings {
        let rendered_lines = report
            .docstrings
            .into_iter()
            .map(|docstring| {
                let name = docstring.host.name.as_deref().unwrap_or("<module>");
                format!(
                    "{}:{}..{} {:?} {} style={} params={} returns={} raises={} blocks={} parsed={}",
                    path.display(),
                    docstring.host.docstring_range.start,
                    docstring.host.docstring_range.end,
                    docstring.host.kind,
                    name,
                    docstring.style,
                    docstring.parameter_count,
                    docstring.return_count,
                    docstring.raise_count,
                    docstring.block_count,
                    docstring.parsed,
                )
            })
            .collect();
        return Ok(FileOutcome {
            path: path.to_path_buf(),
            baseline_key,
            raw_diagnostics: Vec::new(),
            rendered_lines,
            fixed_source: None,
            exit_violation_count: 0,
        });
    }

    let suppression = apply_noqa_suppression_with_report(
        &source,
        settings.filter_diagnostics(report.diagnostics),
        &report.docstrings,
    );
    let mut raw_diagnostics = suppression.diagnostics;
    raw_diagnostics.extend(settings.filter_diagnostics(suppression.noqa_diagnostics));
    if settings.generate_baseline {
        return Ok(FileOutcome {
            path: path.to_path_buf(),
            baseline_key,
            raw_diagnostics,
            rendered_lines: Vec::new(),
            fixed_source: None,
            exit_violation_count: 0,
        });
    }

    let raw_diagnostics_for_baseline = if record_raw_diagnostics {
        raw_diagnostics.clone()
    } else {
        Vec::new()
    };
    let diagnostics = filter_baseline_violations(raw_diagnostics, baseline_data, &baseline_key);
    if settings.fix || settings.diff {
        let outcome = apply_fixes_until_stable(&source, settings.unsafe_fixes, |current_source| {
            let current_report = linter.analyze_source(current_source);
            let current_suppression = apply_noqa_suppression_with_report(
                current_source,
                settings.filter_diagnostics(current_report.diagnostics),
                &current_report.docstrings,
            );
            let mut current_diagnostics = current_suppression.diagnostics;
            current_diagnostics.extend(settings.filter_diagnostics(current_suppression.noqa_diagnostics));
            filter_baseline_violations(current_diagnostics, baseline_data, &baseline_key)
        })?;
        let mut rendered_lines = Vec::new();
        if settings.diff
            && let Some(rendered_diff) = render_diff(path, &source, &outcome.source)
        {
            rendered_lines.push(rendered_diff);
        }
        if !settings.diff {
            let renderer = DiagnosticRenderer::new(path, &source, settings.output_format);
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
            baseline_key,
            raw_diagnostics: raw_diagnostics_for_baseline,
            rendered_lines,
            fixed_source,
            exit_violation_count,
        });
    }

    let renderer = DiagnosticRenderer::new(path, &source, settings.output_format);
    let rendered_lines = diagnostics
        .iter()
        .map(|diagnostic| renderer.render(diagnostic))
        .collect();
    Ok(FileOutcome {
        path: path.to_path_buf(),
        baseline_key,
        raw_diagnostics: raw_diagnostics_for_baseline,
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
