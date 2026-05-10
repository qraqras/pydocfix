use crate::{AnalysisConfig, FileReport, RuleFilter};

/// Library-facing pydocfix analysis engine.
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

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT_PARAMETER_SOURCE: &str = r#"def value(x: int = 42) -> None:
    """Do something.

    Args:
        x (int, optional): The value.
    """
    pass
"#;

    const MISSING_OPTIONAL_SOURCE: &str = r#"def value(x: int = 42) -> None:
    """Do something.

    Args:
        x (int): The value.
    """
    pass
"#;

    #[test]
    fn default_linter_keeps_default_disabled_rules_disabled() {
        let report = Linter::default().analyze_source(DEFAULT_PARAMETER_SOURCE);

        assert!(report.diagnostics.iter().all(|diagnostic| diagnostic.rule != "PRM201"));
        assert!(report.diagnostics.iter().all(|diagnostic| diagnostic.rule != "PRM202"));
    }

    #[test]
    fn rule_filter_enables_prm201_when_selected() {
        let report = Linter::default()
            .with_rule_filter(RuleFilter::new(vec!["PRM201".to_string()], Vec::new()))
            .analyze_source(MISSING_OPTIONAL_SOURCE);

        assert!(report.diagnostics.iter().any(|diagnostic| diagnostic.rule == "PRM201"));
        assert!(report.diagnostics.iter().all(|diagnostic| diagnostic.rule == "PRM201"));
    }

    #[test]
    fn rule_filter_enables_default_disabled_rules_when_selected() {
        let report = Linter::default()
            .with_rule_filter(RuleFilter::new(vec!["PRM202".to_string()], Vec::new()))
            .analyze_source(DEFAULT_PARAMETER_SOURCE);

        assert!(report.diagnostics.iter().any(|diagnostic| diagnostic.rule == "PRM202"));
        assert!(report.diagnostics.iter().all(|diagnostic| diagnostic.rule == "PRM202"));
    }

    #[test]
    fn rule_filter_ignore_takes_precedence_for_default_disabled_rules() {
        let report = Linter::default()
            .with_rule_filter(RuleFilter::new(
                vec!["ALL".to_string()],
                vec!["PRM201".to_string(), "PRM202".to_string()],
            ))
            .analyze_source(DEFAULT_PARAMETER_SOURCE);

        assert!(report.diagnostics.iter().all(|diagnostic| diagnostic.rule != "PRM201"));
        assert!(report.diagnostics.iter().all(|diagnostic| diagnostic.rule != "PRM202"));
    }
}
