use std::env;
use std::path::{Path, PathBuf};

use pydocsync_core::{AnalysisConfig, Diagnostic, Linter, RuleFilter};

use crate::args::CliArgs;
use crate::config::ProjectConfig;

#[derive(Clone, Debug)]
pub(crate) struct ResolvedSettings {
    pub(crate) fix: bool,
    pub(crate) diff: bool,
    pub(crate) unsafe_fixes: bool,
    pub(crate) analysis_config: AnalysisConfig,
    pub(crate) rule_filter: RuleFilter,
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) exclude: Vec<String>,
    pub(crate) project_root: Option<PathBuf>,
    pub(crate) jobs: Option<usize>,
}

impl ResolvedSettings {
    pub(crate) fn resolve(cli_args: CliArgs, project_config: ProjectConfig) -> Self {
        let mut ignore = project_config.ignore.clone();
        ignore.extend(cli_args.ignore.clone());
        let mut exclude = project_config.exclude.clone();
        exclude.extend(cli_args.exclude.clone());
        let project_root = project_config
            .path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .or_else(|| env::current_dir().ok());

        let rule_filter = RuleFilter::new(ignore);

        Self {
            fix: cli_args.fix,
            diff: cli_args.diff,
            unsafe_fixes: cli_args.unsafe_fixes,
            analysis_config: AnalysisConfig,
            rule_filter,
            paths: cli_args.paths,
            exclude,
            project_root,
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
    use pydocsync_core::Range;

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
    fn ignore_combines_cli_and_config() {
        let cli_args = CliArgs {
            fix: false,
            diff: false,
            unsafe_fixes: false,
            jobs: None,
            ignore: vec!["returns".to_string()],
            exclude: Vec::new(),
            paths: vec![PathBuf::from("src")],
        };
        let project_config = ProjectConfig {
            ignore: vec!["args".to_string()],
            ..ProjectConfig::default()
        };

        let settings = ResolvedSettings::resolve(cli_args, project_config);

        assert_eq!(settings.rule_filter.ignore, vec!["args", "returns"]);
        assert_eq!(
            settings
                .filter_diagnostics(vec![
                    diagnostic("args-param-missing"),
                    diagnostic("yields-section-extra")
                ])
                .len(),
            1
        );
    }
}
