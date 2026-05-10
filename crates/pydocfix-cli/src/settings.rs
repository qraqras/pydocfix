use std::env;
use std::path::{Path, PathBuf};

use pydocfix_core::{AnalysisConfig, Diagnostic, Linter, RuleFilter};

use crate::args::CliArgs;
use crate::baseline::resolve_config_path;
use crate::config::{OutputFormat, ProjectConfig};

#[derive(Clone, Debug)]
pub(crate) struct ResolvedSettings {
    pub(crate) debug_docstrings: bool,
    pub(crate) fix: bool,
    pub(crate) diff: bool,
    pub(crate) unsafe_fixes: bool,
    pub(crate) generate_baseline: bool,
    pub(crate) output_format: OutputFormat,
    pub(crate) analysis_config: AnalysisConfig,
    pub(crate) rule_filter: RuleFilter,
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) exclude: Vec<String>,
    pub(crate) project_root: Option<PathBuf>,
    pub(crate) baseline_path: Option<PathBuf>,
    pub(crate) jobs: Option<usize>,
}

impl ResolvedSettings {
    pub(crate) fn resolve(cli_args: CliArgs, project_config: ProjectConfig) -> Self {
        let select = if cli_args.select.is_empty() {
            project_config.select.clone()
        } else {
            cli_args.select.clone()
        };
        let ignore = if cli_args.ignore.is_empty() {
            project_config.ignore.clone()
        } else {
            cli_args.ignore.clone()
        };
        let project_root = project_config
            .path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .or_else(|| env::current_dir().ok());
        let baseline_path = cli_args
            .baseline_path
            .as_deref()
            .or(project_config.baseline.as_deref())
            .map(|path| resolve_config_path(path, project_root.as_deref()));

        let rule_filter = RuleFilter::new(select.clone(), ignore.clone());

        Self {
            debug_docstrings: cli_args.debug_docstrings,
            fix: cli_args.fix,
            diff: cli_args.diff,
            unsafe_fixes: cli_args.unsafe_fixes,
            generate_baseline: cli_args.generate_baseline,
            output_format: cli_args.output_format.unwrap_or(project_config.output_format),
            analysis_config: AnalysisConfig {
                type_annotation_style: cli_args.type_annotation_style.or(project_config.type_annotation_style),
                class_docstring_style: cli_args.class_docstring_style.or(project_config.class_docstring_style),
                allow_optional_shorthand: cli_args.allow_optional_shorthand || project_config.allow_optional_shorthand,
                enable_prm201: false,
                enable_prm202: false,
            },
            rule_filter,
            paths: cli_args.paths,
            exclude: project_config.exclude,
            project_root,
            baseline_path,
            jobs: cli_args.jobs,
        }
    }

    pub(crate) fn filter_diagnostics(&self, diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
        self.rule_filter.filter_diagnostics(diagnostics)
    }

    pub(crate) fn linter(&self) -> Linter {
        Linter::new(self.analysis_config).with_rule_filter(self.rule_filter.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pydocfix_core::Range;

    fn diagnostic(rule: &'static str) -> Diagnostic {
        Diagnostic {
            rule,
            message: String::new(),
            range: Range { start: 0, end: 1 },
            fix: None,
            symbol: None,
        }
    }

    #[test]
    fn cli_select_overrides_config_select() {
        let cli_args = CliArgs {
            debug_docstrings: false,
            fix: false,
            diff: false,
            unsafe_fixes: false,
            generate_baseline: false,
            allow_optional_shorthand: false,
            config_path: None,
            baseline_path: None,
            output_format: None,
            type_annotation_style: None,
            class_docstring_style: None,
            jobs: None,
            select: vec!["SUM".to_string()],
            ignore: Vec::new(),
            paths: vec![PathBuf::from("src")],
        };
        let project_config = ProjectConfig {
            select: vec!["PRM".to_string()],
            ..ProjectConfig::default()
        };

        let settings = ResolvedSettings::resolve(cli_args, project_config);

        assert_eq!(settings.rule_filter.select, vec!["SUM"]);
        assert_eq!(
            settings
                .filter_diagnostics(vec![diagnostic("SUM002"), diagnostic("PRM001")])
                .len(),
            1
        );
    }

    #[test]
    fn all_select_enables_default_disabled_prm_rules() {
        let cli_args = CliArgs {
            debug_docstrings: false,
            fix: false,
            diff: false,
            unsafe_fixes: false,
            generate_baseline: false,
            allow_optional_shorthand: false,
            config_path: None,
            baseline_path: None,
            output_format: None,
            type_annotation_style: None,
            class_docstring_style: None,
            jobs: None,
            select: vec!["ALL".to_string()],
            ignore: Vec::new(),
            paths: vec![PathBuf::from("src")],
        };

        let settings = ResolvedSettings::resolve(cli_args, ProjectConfig::default());

        assert!(settings.rule_filter.enables("PRM201"));
        assert!(settings.rule_filter.enables("PRM202"));
    }
}
