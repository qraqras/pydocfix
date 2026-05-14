//! Rust core for pydocsync.
//!
//! The rewrite pipeline starts with a lightweight Python source scan, then
//! parses each discovered docstring literal through `docstring-cst`.

use std::sync::Arc;

use docstring_cst::Source;
use pydocsync_scanner::{ParameterRecord, summarize_python};

mod linter;
mod model;
mod registry;
mod rules;

pub use linter::Linter;
pub use model::{
    AnalysisConfig, Applicability, Diagnostic, DocstringHost, Edit, FileReport, Fix, ParsedDocstring, Range,
    RuleFilter, SignatureParameter,
};
pub use registry::{RULES, RuleMetadata, is_known_rule};

/// Analyze Python source with the first Rust rewrite pipeline.
pub fn analyze_source(source: &str) -> FileReport {
    Linter::default().analyze_source(source)
}

/// Analyze Python source with explicit rule configuration.
pub fn analyze_source_with_config(source: &str, config: AnalysisConfig) -> FileReport {
    Linter::new(config).analyze_source(source)
}

pub(crate) fn analyze_source_with_filter(source: &str, config: AnalysisConfig, rule_filter: &RuleFilter) -> FileReport {
    let summary = summarize_python(source);
    let source_buffer = Source::new(Arc::<str>::from(source));
    let mut hosts = Vec::new();

    for function_item in summary.items {
        if let Some(range) = function_item.docstring_range {
            let signature_parameters = function_item
                .parameters
                .iter()
                .map(|record| signature_parameter_from_record(source, record))
                .collect();
            hosts.push(DocstringHost {
                name: Some(function_item.name),
                docstring_range: range.into(),
                return_annotation_range: function_item.return_annotation_range.map(Range::from),
                has_return_value: function_item.has_return_value,
                has_yield: function_item.has_yield,
                signature_parameters,
            });
        }
    }

    let mut docstrings = Vec::new();
    let mut diagnostics = Vec::new();

    for host in hosts {
        let cst = source_buffer.parse(host.docstring_range.start, host.docstring_range.end);
        if let Some(cst) = cst {
            let semantic = cst.semantic();
            diagnostics.extend(rules::returns::check_return_rules(
                &source_buffer,
                &host,
                &semantic,
                config,
            ));
            diagnostics.extend(rules::parameters::check_parameter_rules(
                &source_buffer,
                &host,
                &semantic,
                config,
            ));
            docstrings.push(ParsedDocstring {
                host: host.clone(),
                style: format!("{:?}", semantic.style()),
                parsed: true,
                parameter_count: semantic
                    .parameters()
                    .iter()
                    .map(|parameter| parameter.name_ranges.len())
                    .sum(),
                return_count: semantic.returns().len(),
                block_count: semantic.blocks().len(),
            });
        } else {
            docstrings.push(ParsedDocstring {
                host: host.clone(),
                style: "Invalid".to_string(),
                parsed: false,
                parameter_count: 0,
                return_count: 0,
                block_count: 0,
            });
        }
    }

    FileReport {
        docstrings,
        diagnostics: rule_filter.filter_diagnostics(diagnostics),
    }
}

fn signature_parameter_from_record(source: &str, record: &ParameterRecord) -> SignatureParameter {
    SignatureParameter {
        name: record.name.clone(),
        bare_name: record.bare_name.clone(),
        annotation: record
            .annotation_range
            .and_then(|range| source.get(range.start()..range.end()))
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string),
        default_value: record
            .default_range
            .and_then(|range| source.get(range.start()..range.end()))
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string),
        is_vararg: record.is_vararg,
        is_kwarg: record.is_kwarg,
        is_implicit_receiver: record.is_implicit_receiver,
    }
}

pub(crate) fn line_indent_before(source: &str, offset: usize) -> &str {
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    &source[line_start..offset]
}
