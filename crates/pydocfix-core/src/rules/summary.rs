use docstring_cst::semantic::SemanticView;
use docstring_cst::{Source, TextRange};

use crate::{AnalysisConfig, Applicability, Diagnostic, DocstringHost, Edit, Fix};

use super::RuleContext;

pub(crate) fn check_summary_rules(
    source: &Source,
    host: &DocstringHost,
    semantic: &SemanticView,
    config: AnalysisConfig,
) -> Vec<Diagnostic> {
    let ctx = RuleContext {
        source,
        host,
        semantic,
        config,
    };
    let mut diagnostics = Vec::new();
    sum001(&ctx, &mut diagnostics);
    if diagnostics.is_empty() {
        sum002(&ctx, &mut diagnostics);
    }
    diagnostics
}

fn sum001(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let Some(summary) = ctx.semantic.summary() else {
        diagnostics.push(missing_summary_diagnostic(ctx.host));
        return;
    };
    let summary_text = ctx.source.slice(summary.entry_range).unwrap_or("").trim();
    if !summary_text.is_empty() {
        return;
    }
    diagnostics.push(missing_summary_diagnostic(ctx.host));
}

fn sum002(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let Some(summary) = ctx.semantic.summary() else {
        return;
    };
    let summary_range = summary.entry_range;
    let summary_text = ctx.source.slice(summary_range).unwrap_or("").trim();
    if summary_text.is_empty() || matches!(summary_text.chars().next_back(), Some('.' | '!' | '?')) {
        return;
    }
    diagnostics.push(Diagnostic {
        rule: "SUM002",
        message: "Summary should end with a period.".to_string(),
        range: summary_range.into(),
        fix: Some(Fix {
            edits: vec![Edit::insert(summary_insert_offset(ctx.source, summary_range), ".")],
            applicability: Applicability::Safe,
        }),
        symbol: ctx.host.name.clone(),
    });
}

fn missing_summary_diagnostic(host: &DocstringHost) -> Diagnostic {
    Diagnostic {
        rule: "SUM001",
        message: "Docstring has no summary line.".to_string(),
        range: host.docstring_range,
        fix: None,
        symbol: host.name.clone(),
    }
}

fn summary_insert_offset(source: &Source, range: TextRange) -> usize {
    let Some(text) = source.slice(range) else {
        return range.end();
    };
    let trailing_whitespace_len = text.len() - text.trim_end().len();
    range.end() - trailing_whitespace_len
}
