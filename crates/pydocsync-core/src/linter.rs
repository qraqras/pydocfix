use crate::{AnalysisConfig, FileReport, RuleFilter};

/// Library-facing pydocsync analysis engine.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Linter {
    config: AnalysisConfig,
    rule_filter: RuleFilter,
}

impl Linter {
    /// Create a linter with the provided analysis configuration.
    pub fn new(config: AnalysisConfig) -> Self {
        Self {
            config,
            rule_filter: RuleFilter::default(),
        }
    }

    /// Replace the linter's rule filter.
    pub fn with_rule_filter(mut self, rule_filter: RuleFilter) -> Self {
        self.rule_filter = rule_filter;
        self
    }

    /// Analyze Python source and return a file-level report.
    pub fn analyze_source(&self, source: &str) -> FileReport {
        crate::analyze_source_with_filter(source, self.config, &self.rule_filter)
    }

    /// Access the base analysis configuration.
    pub fn config(&self) -> AnalysisConfig {
        self.config
    }

    /// Access the rule filter used by this linter.
    pub fn rule_filter(&self) -> &RuleFilter {
        &self.rule_filter
    }
}
