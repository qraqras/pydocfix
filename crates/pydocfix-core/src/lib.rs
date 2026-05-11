//! Rust core for pydocfix.
//!
//! The rewrite pipeline starts with a lightweight Python source scan, then
//! parses each discovered docstring literal through `docstring-cst`.

use std::sync::Arc;

use docstring_cst::semantic::SemanticBlock;
use docstring_cst::{DocstringStyle, Source, TextRange};
use pydocfix_scanner::{Item, ParameterRecord, RaiseRecord, summarize_python};

mod linter;
mod model;
mod registry;
mod rules;

pub use linter::Linter;
pub use model::{
    AnalysisConfig, Applicability, ClassDocstringStyle, Diagnostic, DocstringHost, Edit, FileReport, Fix, HostKind,
    ParsedDocstring, RaisedException, Range, RuleFilter, SignatureParameter, TypeAnnotationStyle,
};
pub use registry::{RULES, RuleMetadata, is_default_disabled_rule, is_known_rule};

/// Analyze Python source with the first Rust rewrite pipeline.
pub fn analyze_source(source: &str) -> FileReport {
    Linter::default().analyze_source(source)
}

/// Analyze Python source with explicit rule configuration.
pub fn analyze_source_with_config(source: &str, config: AnalysisConfig) -> FileReport {
    Linter::new(config).analyze_source(source)
}

pub(crate) fn analyze_source_with_filter(source: &str, config: AnalysisConfig, rule_filter: &RuleFilter) -> FileReport {
    let config = AnalysisConfig {
        enable_prm201: config.enable_prm201 || rule_filter.enables_default_disabled("PRM201"),
        enable_prm202: config.enable_prm202 || rule_filter.enables_default_disabled("PRM202"),
        ..config
    };
    let summary = summarize_python(source);
    let source_buffer = Source::new(Arc::<str>::from(source));
    let mut hosts = Vec::new();

    if let Some(range) = summary.module_docstring {
        hosts.push(DocstringHost {
            kind: HostKind::Module,
            name: None,
            docstring_range: range.into(),
            parent_index: None,
            parent_class_docstring_range: None,
            return_annotation_range: None,
            has_return_value: false,
            has_yield: false,
            raised_exceptions: Vec::new(),
            signature_parameters: Vec::new(),
        });
    }

    let items = summary.items;
    let class_docstring_ranges: Vec<Option<Range>> = items
        .iter()
        .map(|item| match item {
            Item::Class(class_item) => class_item.docstring_range.map(Range::from),
            Item::Function(_) => None,
        })
        .collect();
    let mut class_init_parameters: Vec<Option<Vec<SignatureParameter>>> = vec![None; items.len()];
    let mut class_init_raises: Vec<Option<Vec<RaisedException>>> = vec![None; items.len()];
    for item in &items {
        let Item::Function(function_item) = item else {
            continue;
        };
        if function_item.name != "__init__" {
            continue;
        }
        let Some(parent_index) = function_item.parent_index else {
            continue;
        };
        if parent_index >= items.len() {
            continue;
        }
        class_init_parameters[parent_index] = Some(
            function_item
                .parameters
                .iter()
                .map(|record| signature_parameter_from_record(source, record))
                .collect(),
        );
        class_init_raises[parent_index] = Some(
            function_item
                .raises
                .iter()
                .filter_map(|record| raised_exception_from_record(source, record))
                .collect(),
        );
    }

    for (item_index, item) in items.into_iter().enumerate() {
        match item {
            Item::Class(class_item) => {
                if let Some(range) = class_item.docstring_range {
                    hosts.push(DocstringHost {
                        kind: HostKind::Class,
                        name: Some(class_item.name),
                        docstring_range: range.into(),
                        parent_index: class_item.parent_index,
                        parent_class_docstring_range: class_item
                            .parent_index
                            .and_then(|index| class_docstring_ranges.get(index).copied().flatten()),
                        return_annotation_range: None,
                        has_return_value: false,
                        has_yield: false,
                        raised_exceptions: class_init_raises
                            .get(item_index)
                            .and_then(|raises| raises.clone())
                            .unwrap_or_default(),
                        signature_parameters: class_init_parameters
                            .get(item_index)
                            .and_then(|parameters| parameters.clone())
                            .unwrap_or_default(),
                    });
                }
            }
            Item::Function(function_item) => {
                if let Some(range) = function_item.docstring_range {
                    let raised_exceptions = function_item
                        .raises
                        .iter()
                        .filter_map(|record| raised_exception_from_record(source, record))
                        .collect();
                    let signature_parameters = function_item
                        .parameters
                        .iter()
                        .map(|record| signature_parameter_from_record(source, record))
                        .collect();
                    hosts.push(DocstringHost {
                        kind: HostKind::Function,
                        name: Some(function_item.name),
                        docstring_range: range.into(),
                        parent_index: function_item.parent_index,
                        parent_class_docstring_range: function_item
                            .parent_index
                            .and_then(|index| class_docstring_ranges.get(index).copied().flatten()),
                        return_annotation_range: function_item.return_annotation_range.map(Range::from),
                        has_return_value: function_item.has_return_value,
                        has_yield: function_item.has_yield,
                        raised_exceptions,
                        signature_parameters,
                    });
                }
            }
        }
    }

    let mut docstrings = Vec::new();
    let mut diagnostics = Vec::new();

    for host in hosts {
        let cst = source_buffer.parse(host.docstring_range.start, host.docstring_range.end);
        if let Some(cst) = cst {
            let semantic = cst.semantic();
            diagnostics.extend(rules::summary::check_summary_rules(
                &source_buffer,
                &host,
                &semantic,
                config,
            ));
            diagnostics.extend(rules::returns::check_return_rules(
                &source_buffer,
                &host,
                &semantic,
                config,
            ));
            diagnostics.extend(rules::yields::check_yield_rules(
                &source_buffer,
                &host,
                &semantic,
                config,
            ));
            diagnostics.extend(rules::raises::check_raise_rules(
                &source_buffer,
                &host,
                &semantic,
                config,
            ));
            diagnostics.extend(rules::documentation::check_doc_rules(
                &source_buffer,
                &host,
                &semantic,
                config,
            ));
            diagnostics.extend(rules::classes::check_class_rules(
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
                parameter_count: semantic.parameters().len(),
                return_count: semantic.returns().len(),
                raise_count: semantic.raises().len(),
                block_count: semantic.blocks().len(),
            });
        } else {
            docstrings.push(ParsedDocstring {
                host: host.clone(),
                style: "Invalid".to_string(),
                parsed: false,
                parameter_count: 0,
                return_count: 0,
                raise_count: 0,
                block_count: 0,
            });
        }
    }

    FileReport {
        docstrings,
        diagnostics: rule_filter.filter_diagnostics(diagnostics),
    }
}

pub(crate) fn is_short_plain_docstring(semantic: &docstring_cst::semantic::SemanticView) -> bool {
    semantic.style() == DocstringStyle::Plain && semantic.extended_summary().is_none() && semantic.blocks().is_empty()
}

pub(crate) fn missing_section_diagnostic(
    rule: &'static str,
    message: &str,
    _source: &Source,
    host: &DocstringHost,
    semantic: &docstring_cst::semantic::SemanticView,
    replacement: String,
) -> Diagnostic {
    let insert_offset = semantic
        .close_quote()
        .map(|quote| quote.entry_range.start())
        .unwrap_or(host.docstring_range.end);
    Diagnostic {
        rule,
        message: message.to_string(),
        range: semantic
            .summary()
            .map(|summary| summary.entry_range.into())
            .unwrap_or(host.docstring_range),
        fix: Some(Fix {
            edits: vec![Edit::insert(insert_offset, replacement)],
            applicability: Applicability::Unsafe,
        }),
        symbol: host.name.clone(),
    }
}

pub(crate) fn section_diagnostic(
    rule: &'static str,
    message: &str,
    host: &DocstringHost,
    block: &SemanticBlock,
    applicability: Applicability,
) -> Diagnostic {
    Diagnostic {
        rule,
        message: message.to_string(),
        range: block.name_range.into(),
        fix: Some(Fix {
            edits: vec![Edit {
                range: block.entry_range.into(),
                replacement: String::new(),
            }],
            applicability,
        }),
        symbol: host.name.clone(),
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

fn raised_exception_from_record(source: &str, record: &RaiseRecord) -> Option<RaisedException> {
    let range: Range = record.name_range.into();
    let name = source.get(range.start..range.end)?.trim().to_string();
    (!name.is_empty()).then_some(RaisedException {
        name,
        range,
        from_bare_except: record.from_bare_except,
    })
}

pub(crate) fn unique_raised_exception_names(host: &DocstringHost) -> Vec<&str> {
    let mut names = Vec::new();
    for exception in &host.raised_exceptions {
        let name = bare_exception_name(&exception.name);
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

pub(crate) fn bare_exception_name(name: &str) -> &str {
    name.trim().rsplit('.').next().unwrap_or(name.trim())
}

pub(crate) fn raises_section_stub(source: &Source, host: &DocstringHost, style: DocstringStyle) -> String {
    let indent = line_indent_before(source.source(), host.docstring_range.start);
    let names = unique_raised_exception_names(host);
    match style {
        DocstringStyle::Numpy => {
            let mut stub = format!("\n\n{indent}Raises\n{indent}------");
            for name in names {
                stub.push_str(&format!("\n{indent}{name}"));
            }
            stub.push_str(&format!("\n{indent}"));
            stub
        }
        _ => {
            let mut stub = format!("\n\n{indent}Raises:");
            for name in names {
                stub.push_str(&format!("\n{indent}    {name}:"));
            }
            stub.push_str(&format!("\n{indent}"));
            stub
        }
    }
}

pub(crate) fn raises_entry_append_text(
    source: &Source,
    header_range: TextRange,
    style: DocstringStyle,
    exception_name: &str,
) -> String {
    let header_indent = line_indent_before(source.source(), header_range.start());
    match style {
        DocstringStyle::Numpy => format!("\n{header_indent}{exception_name}"),
        _ => format!("\n{header_indent}    {exception_name}:"),
    }
}

pub(crate) fn return_annotation<'a>(source: &'a Source, host: &DocstringHost) -> Option<&'a str> {
    let range = host.return_annotation_range?;
    let text = source.slice(range.into_text_range())?.trim();
    let annotation = text.strip_prefix("->")?.trim();
    (!annotation.is_empty()).then_some(annotation)
}

pub(crate) fn meaningful_return_annotation<'a>(source: &'a Source, host: &DocstringHost) -> Option<&'a str> {
    let annotation = return_annotation(source, host)?;
    (!annotation.is_empty() && annotation != "None").then_some(annotation)
}

pub(crate) fn types_match(docstring_type: &str, signature_type: &str) -> bool {
    normalize_type_for_comparison(docstring_type) == normalize_type_for_comparison(signature_type)
}

fn normalize_type_for_comparison(type_text: &str) -> String {
    type_text
        .trim()
        .trim_matches('`')
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

pub(crate) fn return_type_insert_text(style: DocstringStyle, signature_type: &str) -> String {
    match style {
        DocstringStyle::Numpy => format!("{signature_type}\n"),
        _ => format!("{signature_type}: "),
    }
}

pub(crate) fn redundant_type_delete_range(source: &Source, style: DocstringStyle, type_range: TextRange) -> Range {
    let bytes = source.source().as_bytes();
    let mut end = type_range.end();
    match style {
        DocstringStyle::Numpy => {
            if let Some(relative_newline) = bytes[end..].iter().position(|byte| *byte == b'\n') {
                end += relative_newline + 1;
            }
        }
        _ => {
            if bytes.get(end) == Some(&b':') {
                end += 1;
                if bytes.get(end) == Some(&b' ') {
                    end += 1;
                }
            }
        }
    }
    Range {
        start: type_range.start(),
        end,
    }
}

pub(crate) fn returns_section_stub(
    source: &Source,
    host: &DocstringHost,
    style: DocstringStyle,
    return_annotation: &str,
) -> String {
    let indent = line_indent_before(source.source(), host.docstring_range.start);
    match style {
        DocstringStyle::Numpy => {
            format!("\n\n{indent}Returns\n{indent}-------\n{indent}{return_annotation}\n{indent}    TODO.\n{indent}")
        }
        _ => format!("\n\n{indent}Returns:\n{indent}    {return_annotation}: TODO.\n{indent}"),
    }
}

pub(crate) fn yields_section_stub(
    source: &Source,
    host: &DocstringHost,
    style: DocstringStyle,
    yield_type: Option<&str>,
) -> String {
    let indent = line_indent_before(source.source(), host.docstring_range.start);
    let yield_type = yield_type.unwrap_or("value");
    match style {
        DocstringStyle::Numpy => {
            format!("\n\n{indent}Yields\n{indent}------\n{indent}{yield_type}\n{indent}    TODO.\n{indent}")
        }
        _ => format!("\n\n{indent}Yields:\n{indent}    {yield_type}: TODO.\n{indent}"),
    }
}

pub(crate) fn yield_type_annotation<'a>(source: &'a Source, host: &DocstringHost) -> Option<&'a str> {
    let range = host.return_annotation_range?;
    let text = source.slice(range.into_text_range())?.trim();
    let annotation = text.strip_prefix("->")?.trim();
    extract_yield_type(annotation)
}

fn extract_yield_type(annotation: &str) -> Option<&str> {
    let open = annotation.find('[')?;
    let close = annotation.rfind(']')?;
    let base = annotation[..open].rsplit('.').next()?.trim();
    if !matches!(
        base,
        "Generator" | "Iterator" | "Iterable" | "AsyncGenerator" | "AsyncIterator" | "AsyncIterable"
    ) {
        return None;
    }
    let inner = &annotation[open + 1..close];
    let first = inner.split(',').next()?.trim();
    (!first.is_empty()).then_some(first)
}

pub(crate) fn line_indent_before(source: &str, offset: usize) -> &str {
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    &source[line_start..offset]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyzes_module_class_and_function_docstrings() {
        let source = r#""""Module docs."""

class Example:
    """Class docs."""

    def method(self, name: str) -> str:
        """Greet.

        Args:
            name: Person to greet.

        Returns:
            str: Greeting text.
        """
        return name
"#;

        let report = analyze_source(source);
        assert_eq!(report.docstrings.len(), 3);
        assert_eq!(report.docstrings[0].host.kind, HostKind::Module);
        assert_eq!(report.docstrings[1].host.name.as_deref(), Some("Example"));
        assert_eq!(report.docstrings[2].host.name.as_deref(), Some("method"));
        assert!(report.docstrings.iter().all(|docstring| docstring.parsed));
        assert_eq!(report.docstrings[2].parameter_count, 1);
        assert_eq!(report.docstrings[2].return_count, 1);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn emits_sum001_for_empty_docstring() {
        let source = r#"def empty():
    """"""
    return None
"#;

        let report = analyze_source(source);
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].rule, "SUM001");
        assert_eq!(report.diagnostics[0].symbol.as_deref(), Some("empty"));
        assert!(report.diagnostics[0].fix.is_none());
    }

    #[test]
    fn emits_sum001_when_docstring_starts_with_section() {
        let source = r#"def documented_args():
    """
    Args:
        value: Input value.
    """
    return None
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "SUM001");
        assert!(diagnostic.fix.is_none());
    }

    #[test]
    fn emits_sum002_safe_fix_for_summary_without_period() {
        let source = r#"def greet():
    """Greet someone"""
    return None
"#;

        let report = analyze_source(source);
        assert_eq!(report.diagnostics.len(), 1);
        let diagnostic = &report.diagnostics[0];
        assert_eq!(diagnostic.rule, "SUM002");
        let fix = diagnostic.fix.as_ref().expect("SUM002 should be fixable");
        assert_eq!(fix.applicability, Applicability::Safe);
        assert_eq!(fix.edits.len(), 1);
        assert_eq!(fix.edits[0].replacement, ".");
        assert_eq!(
            &source[fix.edits[0].range.start - 6..fix.edits[0].range.start],
            "omeone"
        );
    }

    #[test]
    fn emits_rtn001_for_annotated_function_missing_returns_section() {
        let source = r#"def value() -> int:
    """Return a value."""
    return 1
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "RTN001");
        assert_eq!(diagnostic.symbol.as_deref(), Some("value"));
        let fix = diagnostic.fix.as_ref().expect("RTN001 should have an unsafe stub fix");
        assert_eq!(fix.applicability, Applicability::Unsafe);
        assert_eq!(fix.edits.len(), 1);
        assert!(fix.edits[0].replacement.contains("Returns:"));
        assert!(fix.edits[0].replacement.contains("int: TODO."));
    }

    #[test]
    fn emits_rtn002_for_returns_section_without_value_return() {
        let source = r#"def noop() -> None:
    """Do nothing.

    Returns:
        None: Nothing.
    """
    return None
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "RTN002");
        assert_eq!(diagnostic.symbol.as_deref(), Some("noop"));
        let fix = diagnostic.fix.as_ref().expect("RTN002 should delete the section");
        assert_eq!(fix.applicability, Applicability::Safe);
        assert_eq!(fix.edits.len(), 1);
        assert_eq!(fix.edits[0].replacement, "");
        let deleted_text = &source[fix.edits[0].range.start..fix.edits[0].range.end];
        assert!(deleted_text.contains("Returns:"));
    }

    #[test]
    fn emits_rtn003_for_returns_entry_without_description() {
        let source = r#"def value() -> int:
    """Return a value.

    Returns:
        int:
    """
    return 1
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "RTN003");
        assert_eq!(diagnostic.symbol.as_deref(), Some("value"));
        assert!(diagnostic.fix.is_none());
    }

    #[test]
    fn emits_rtn101_for_return_type_mismatch() {
        let source = r#"def value() -> int:
    """Return a value.

    Returns:
        str: Wrong type.
    """
    return 1
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "RTN101");
        assert_eq!(diagnostic.symbol.as_deref(), Some("value"));
        assert_eq!(
            diagnostic.message,
            "Docstring return type 'str' does not match type hint 'int'."
        );
        let fix = diagnostic.fix.as_ref().expect("RTN101 should replace the type");
        assert_eq!(fix.applicability, Applicability::Unsafe);
        assert_eq!(fix.edits.len(), 1);
        assert_eq!(fix.edits[0].replacement, "int");
    }

    #[test]
    fn emits_rtn102_when_return_type_is_missing_everywhere() {
        let source = r#"def value():
    """Return a value.

    Returns:
        The value.
    """
    return 1
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "RTN102");
        assert_eq!(diagnostic.symbol.as_deref(), Some("value"));
        assert!(diagnostic.fix.is_none());
    }

    #[test]
    fn emits_rtn103_for_docstring_style_missing_docstring_type() {
        let source = r#"def value() -> int:
    """Return a value.

    Returns:
        The value.
    """
    return 1
"#;

        let report = analyze_source_with_type_style(source, TypeAnnotationStyle::Docstring);
        let diagnostic = single_rule(&report, "RTN103");
        assert_eq!(diagnostic.symbol.as_deref(), Some("value"));
        let fix = diagnostic.fix.as_ref().expect("RTN103 should use the signature type");
        assert_eq!(fix.applicability, Applicability::Unsafe);
        assert_eq!(fix.edits[0].replacement, "int: ");
    }

    #[test]
    fn emits_rtn104_for_signature_style_redundant_docstring_type() {
        let source = r#"def value() -> int:
    """Return a value.

    Returns:
        int: The value.
    """
    return 1
"#;

        let report = analyze_source_with_type_style(source, TypeAnnotationStyle::Signature);
        let diagnostic = single_rule(&report, "RTN104");
        assert_eq!(diagnostic.symbol.as_deref(), Some("value"));
        let fix = diagnostic
            .fix
            .as_ref()
            .expect("RTN104 should delete the redundant type");
        assert_eq!(fix.applicability, Applicability::Safe);
        assert_eq!(&source[fix.edits[0].range.start..fix.edits[0].range.end], "int: ");
    }

    #[test]
    fn emits_rtn105_for_signature_style_missing_signature_type() {
        let source = r#"def value():
    """Return a value.

    Returns:
        The value.
    """
    return 1
"#;

        let report = analyze_source_with_type_style(source, TypeAnnotationStyle::Signature);
        let diagnostic = single_rule(&report, "RTN105");
        assert_eq!(diagnostic.symbol.as_deref(), Some("value"));
        assert!(diagnostic.fix.is_none());
        assert!(report.diagnostics.iter().all(|diagnostic| diagnostic.rule != "RTN102"));
    }

    #[test]
    fn emits_rtn106_for_docstring_style_signature_type() {
        let source = r#"def value() -> int:
    """Return a value.

    Returns:
        int: The value.
    """
    return 1
"#;

        let report = analyze_source_with_type_style(source, TypeAnnotationStyle::Docstring);
        let diagnostic = single_rule(&report, "RTN106");
        assert_eq!(diagnostic.symbol.as_deref(), Some("value"));
        assert!(diagnostic.fix.is_none());
    }

    #[test]
    fn emits_yld001_for_generator_missing_yields_section() {
        let source = r#"from collections.abc import Iterator

def values() -> Iterator[int]:
    """Generate values."""
    yield 1
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "YLD001");
        assert_eq!(diagnostic.symbol.as_deref(), Some("values"));
        let fix = diagnostic.fix.as_ref().expect("YLD001 should have an unsafe stub fix");
        assert_eq!(fix.applicability, Applicability::Unsafe);
        assert_eq!(fix.edits.len(), 1);
        assert!(fix.edits[0].replacement.contains("Yields:"));
        assert!(fix.edits[0].replacement.contains("int: TODO."));
        assert!(report.diagnostics.iter().all(|diagnostic| diagnostic.rule != "RTN001"));
    }

    #[test]
    fn emits_yld002_for_yields_section_in_non_generator() {
        let source = r#"def noop() -> None:
    """Do nothing.

    Yields:
        int: Nothing.
    """
    return None
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "YLD002");
        assert_eq!(diagnostic.symbol.as_deref(), Some("noop"));
        let fix = diagnostic.fix.as_ref().expect("YLD002 should delete the section");
        assert_eq!(fix.applicability, Applicability::Safe);
        assert_eq!(fix.edits.len(), 1);
        assert_eq!(fix.edits[0].replacement, "");
        let deleted_text = &source[fix.edits[0].range.start..fix.edits[0].range.end];
        assert!(deleted_text.contains("Yields:"));
    }

    #[test]
    fn emits_yld003_for_yields_entry_without_description() {
        let source = r#"from collections.abc import Iterator

def values() -> Iterator[int]:
    """Generate values.

    Yields:
        int:
    """
    yield 1
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "YLD003");
        assert_eq!(diagnostic.symbol.as_deref(), Some("values"));
        assert!(diagnostic.fix.is_none());
    }

    #[test]
    fn emits_yld101_for_yield_type_mismatch() {
        let source = r#"from collections.abc import Iterator

def values() -> Iterator[int]:
    """Generate values.

    Yields:
        str: Wrong type.
    """
    yield 1
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "YLD101");
        assert_eq!(diagnostic.symbol.as_deref(), Some("values"));
        assert_eq!(
            diagnostic.message,
            "Docstring yield type 'str' does not match type hint 'int'."
        );
        let fix = diagnostic.fix.as_ref().expect("YLD101 should replace the type");
        assert_eq!(fix.applicability, Applicability::Unsafe);
        assert_eq!(fix.edits.len(), 1);
        assert_eq!(fix.edits[0].replacement, "int");
    }

    #[test]
    fn emits_yld102_when_yield_type_is_missing_everywhere() {
        let source = r#"def values():
    """Generate values.

    Yields:
        The value.
    """
    yield 1
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "YLD102");
        assert_eq!(diagnostic.symbol.as_deref(), Some("values"));
        assert!(diagnostic.fix.is_none());
    }

    #[test]
    fn emits_yld103_for_docstring_style_missing_docstring_type() {
        let source = r#"from collections.abc import Iterator

def values() -> Iterator[int]:
    """Generate values.

    Yields:
        The value.
    """
    yield 1
"#;

        let report = analyze_source_with_type_style(source, TypeAnnotationStyle::Docstring);
        let diagnostic = single_rule(&report, "YLD103");
        assert_eq!(diagnostic.symbol.as_deref(), Some("values"));
        let fix = diagnostic.fix.as_ref().expect("YLD103 should use the signature type");
        assert_eq!(fix.applicability, Applicability::Unsafe);
        assert_eq!(fix.edits[0].replacement, "int: ");
    }

    #[test]
    fn emits_yld104_for_signature_style_redundant_docstring_type() {
        let source = r#"from collections.abc import Iterator

def values() -> Iterator[int]:
    """Generate values.

    Yields:
        int: The value.
    """
    yield 1
"#;

        let report = analyze_source_with_type_style(source, TypeAnnotationStyle::Signature);
        let diagnostic = single_rule(&report, "YLD104");
        assert_eq!(diagnostic.symbol.as_deref(), Some("values"));
        let fix = diagnostic
            .fix
            .as_ref()
            .expect("YLD104 should delete the redundant type");
        assert_eq!(fix.applicability, Applicability::Safe);
        assert_eq!(&source[fix.edits[0].range.start..fix.edits[0].range.end], "int: ");
    }

    #[test]
    fn emits_yld105_for_signature_style_missing_signature_type() {
        let source = r#"def values():
    """Generate values.

    Yields:
        The value.
    """
    yield 1
"#;

        let report = analyze_source_with_type_style(source, TypeAnnotationStyle::Signature);
        let diagnostic = single_rule(&report, "YLD105");
        assert_eq!(diagnostic.symbol.as_deref(), Some("values"));
        assert!(diagnostic.fix.is_none());
        assert!(report.diagnostics.iter().all(|diagnostic| diagnostic.rule != "YLD102"));
    }

    #[test]
    fn emits_yld106_for_docstring_style_signature_type() {
        let source = r#"from collections.abc import Iterator

def values() -> Iterator[int]:
    """Generate values.

    Yields:
        int: The value.
    """
    yield 1
"#;

        let report = analyze_source_with_type_style(source, TypeAnnotationStyle::Docstring);
        let diagnostic = single_rule(&report, "YLD106");
        assert_eq!(diagnostic.symbol.as_deref(), Some("values"));
        assert!(diagnostic.fix.is_none());
    }

    #[test]
    fn emits_ris001_for_missing_raises_section() {
        let source = r#"def fail():
    """Do something."""
    raise ValueError("bad")
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "RIS001");
        assert_eq!(diagnostic.symbol.as_deref(), Some("fail"));
        let fix = diagnostic.fix.as_ref().expect("RIS001 should append a stub section");
        assert_eq!(fix.applicability, Applicability::Unsafe);
        assert!(fix.edits[0].replacement.contains("Raises:"));
        assert!(fix.edits[0].replacement.contains("ValueError:"));
    }

    #[test]
    fn emits_ris002_for_unnecessary_raises_section() {
        let source = r#"def ok():
    """Do something.

    Raises:
        ValueError: Never raised.
    """
    return None
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "RIS002");
        assert_eq!(diagnostic.symbol.as_deref(), Some("ok"));
        let fix = diagnostic.fix.as_ref().expect("RIS002 should delete the section");
        assert_eq!(fix.applicability, Applicability::Safe);
        assert_eq!(fix.edits[0].replacement, "");
    }

    #[test]
    fn emits_ris003_for_raises_entry_without_description() {
        let source = r#"def fail():
    """Do something.

    Raises:
        ValueError:
    """
    raise ValueError("bad")
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "RIS003");
        assert_eq!(diagnostic.symbol.as_deref(), Some("fail"));
        assert!(diagnostic.fix.is_none());
    }

    #[test]
    fn emits_ris004_for_undocumented_raised_exception() {
        let source = r#"def fail():
    """Do something.

    Raises:
        TypeError: Wrong type.
    """
    raise ValueError("bad")
    raise TypeError("bad")
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "RIS004");
        assert_eq!(diagnostic.symbol.as_deref(), Some("fail"));
        assert_eq!(
            diagnostic.message,
            "Raised exception 'ValueError' not documented in Raises section."
        );
        let fix = diagnostic.fix.as_ref().expect("RIS004 should append an entry");
        assert_eq!(fix.applicability, Applicability::Unsafe);
        assert_eq!(fix.edits[0].replacement, "\n        ValueError:");
    }

    #[test]
    fn emits_ris005_for_documented_exception_not_raised() {
        let source = r#"def fail():
    """Do something.

    Raises:
        ValueError: Raised.
        TypeError: Never raised.
    """
    raise ValueError("bad")
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "RIS005");
        assert_eq!(diagnostic.symbol.as_deref(), Some("fail"));
        assert_eq!(
            diagnostic.message,
            "Raises entry 'TypeError' not raised in function body."
        );
        let fix = diagnostic.fix.as_ref().expect("RIS005 should delete the entry");
        assert_eq!(fix.applicability, Applicability::Unsafe);
        assert_eq!(fix.edits[0].replacement, "");
    }

    #[test]
    fn emits_doc001_for_sections_out_of_order() {
        let source = r#"def value(x: int) -> int:
    """Do something.

    Returns:
        int: The result.

    Args:
        x: The input.
    """
    return x
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "DOC001");
        assert_eq!(diagnostic.symbol.as_deref(), Some("value"));
        let fix = diagnostic.fix.as_ref().expect("DOC001 should reorder sections");
        assert_eq!(fix.applicability, Applicability::Unsafe);
        assert!(fix.edits[0].replacement.find("Args:").unwrap() < fix.edits[0].replacement.find("Returns:").unwrap());
    }

    #[test]
    fn emits_doc002_for_wrong_entry_indentation() {
        let source = r#"def value(x: int) -> int:
    """Do something.

    Args:
      x: Under-indented.
    """
    return x
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "DOC002");
        assert_eq!(diagnostic.symbol.as_deref(), Some("value"));
        assert_eq!(diagnostic.message, "Expected 8-space indentation, found 6.");
        let fix = diagnostic.fix.as_ref().expect("DOC002 should adjust indentation");
        assert_eq!(fix.applicability, Applicability::Safe);
        assert_eq!(fix.edits[0].replacement, "        ");
    }

    #[test]
    fn emits_doc003_for_summary_only_multiline_docstring() {
        let source = r#"def value():
    """
    Do something.
    """
    return None
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "DOC003");
        assert_eq!(diagnostic.symbol.as_deref(), Some("value"));
        let fix = diagnostic.fix.as_ref().expect("DOC003 should collapse the docstring");
        assert_eq!(fix.applicability, Applicability::Safe);
        assert_eq!(fix.edits[0].replacement, "\"\"\"Do something.\"\"\"");
    }

    #[test]
    fn emits_cls001_when_class_and_init_both_have_docstrings() {
        let source = r#"class Example:
    """Class docs."""

    def __init__(self, value: int) -> None:
        """Initialize."""
        self.value = value
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "CLS001");
        assert_eq!(diagnostic.symbol.as_deref(), Some("__init__"));
        assert_eq!(
            diagnostic.message,
            "__init__ has its own docstring but the class also has a docstring."
        );
        assert!(diagnostic.fix.is_none());
    }

    #[test]
    fn emits_cls101_for_returns_section_in_class_docstring() {
        let source = r#"class Example:
    """Class docs.

    Returns:
        int: A value.
    """
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "CLS101");
        assert_eq!(diagnostic.symbol.as_deref(), Some("Example"));
        assert_eq!(diagnostic.message, "Class docstring should not have a Returns section.");
        assert_eq!(diagnostic.fix.as_ref().unwrap().applicability, Applicability::Safe);
    }

    #[test]
    fn emits_cls102_for_yields_section_in_class_docstring() {
        let source = r#"class Example:
    """Class docs.

    Yields:
        int: A value.
    """
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "CLS102");
        assert_eq!(diagnostic.symbol.as_deref(), Some("Example"));
        assert_eq!(diagnostic.message, "Class docstring should not have a Yields section.");
        assert_eq!(diagnostic.fix.as_ref().unwrap().applicability, Applicability::Safe);
    }

    #[test]
    fn emits_cls201_for_returns_section_in_init_docstring() {
        let source = r#"class Example:
    def __init__(self) -> None:
        """Initialize.

        Returns:
            None: Nothing.
        """
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "CLS201");
        assert_eq!(diagnostic.symbol.as_deref(), Some("__init__"));
        assert_eq!(
            diagnostic.message,
            "__init__ docstring should not have a Returns section."
        );
        assert_eq!(diagnostic.fix.as_ref().unwrap().applicability, Applicability::Safe);
    }

    #[test]
    fn emits_cls202_for_yields_section_in_init_docstring() {
        let source = r#"class Example:
    def __init__(self) -> None:
        """Initialize.

        Yields:
            None: Nothing.
        """
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "CLS202");
        assert_eq!(diagnostic.symbol.as_deref(), Some("__init__"));
        assert_eq!(
            diagnostic.message,
            "__init__ docstring should not have a Yields section."
        );
        assert_eq!(diagnostic.fix.as_ref().unwrap().applicability, Applicability::Safe);
    }

    #[test]
    fn emits_cls103_for_class_args_section_in_init_style() {
        let source = r#"class Example:
    """Class docs.

    Args:
        value: The value.
    """

    def __init__(self, value: int) -> None:
        self.value = value
"#;

        let report = analyze_source_with_class_style(source, ClassDocstringStyle::Init);
        let diagnostic = single_rule(&report, "CLS103");
        assert_eq!(diagnostic.symbol.as_deref(), Some("Example"));
        assert_eq!(diagnostic.fix.as_ref().unwrap().applicability, Applicability::Unsafe);
    }

    #[test]
    fn emits_cls104_for_class_raises_section_in_init_style() {
        let source = r#"class Example:
    """Class docs.

    Raises:
        ValueError: Bad value.
    """

    def __init__(self, value: int) -> None:
        raise ValueError("bad")
"#;

        let report = analyze_source_with_class_style(source, ClassDocstringStyle::Init);
        let diagnostic = single_rule(&report, "CLS104");
        assert_eq!(diagnostic.symbol.as_deref(), Some("Example"));
        assert_eq!(diagnostic.fix.as_ref().unwrap().applicability, Applicability::Unsafe);
    }

    #[test]
    fn emits_cls105_for_class_style_missing_class_args_section() {
        let source = r#"class Example:
    """Class docs.

    Details.
    """

    def __init__(self, value: int) -> None:
        self.value = value
"#;

        let report = analyze_source_with_class_style(source, ClassDocstringStyle::Class);
        let diagnostic = single_rule(&report, "CLS105");
        assert_eq!(diagnostic.symbol.as_deref(), Some("Example"));
        assert!(diagnostic.fix.as_ref().unwrap().edits[0].replacement.contains("Args:"));
        assert!(
            diagnostic.fix.as_ref().unwrap().edits[0]
                .replacement
                .contains("value (int):")
        );
    }

    #[test]
    fn emits_cls106_for_class_style_missing_class_raises_section() {
        let source = r#"class Example:
    """Class docs.

    Args:
        value: The value.
    """

    def __init__(self, value: int) -> None:
        raise ValueError("bad")
"#;

        let report = analyze_source_with_class_style(source, ClassDocstringStyle::Class);
        let diagnostic = single_rule(&report, "CLS106");
        assert_eq!(diagnostic.symbol.as_deref(), Some("Example"));
        assert!(
            diagnostic.fix.as_ref().unwrap().edits[0]
                .replacement
                .contains("Raises:")
        );
        assert!(
            diagnostic.fix.as_ref().unwrap().edits[0]
                .replacement
                .contains("ValueError:")
        );
    }

    #[test]
    fn emits_cls203_for_init_args_section_in_class_style() {
        let source = r#"class Example:
    def __init__(self, value: int) -> None:
        """Initialize.

        Args:
            value: The value.
        """
"#;

        let report = analyze_source_with_class_style(source, ClassDocstringStyle::Class);
        let diagnostic = single_rule(&report, "CLS203");
        assert_eq!(diagnostic.symbol.as_deref(), Some("__init__"));
        assert_eq!(diagnostic.fix.as_ref().unwrap().applicability, Applicability::Unsafe);
    }

    #[test]
    fn emits_cls204_for_init_raises_section_in_class_style() {
        let source = r#"class Example:
    def __init__(self, value: int) -> None:
        """Initialize.

        Raises:
            ValueError: Bad value.
        """
        raise ValueError("bad")
"#;

        let report = analyze_source_with_class_style(source, ClassDocstringStyle::Class);
        let diagnostic = single_rule(&report, "CLS204");
        assert_eq!(diagnostic.symbol.as_deref(), Some("__init__"));
        assert_eq!(diagnostic.fix.as_ref().unwrap().applicability, Applicability::Unsafe);
    }

    #[test]
    fn emits_cls205_for_init_style_missing_init_args_section() {
        let source = r#"class Example:
    def __init__(self, value: int) -> None:
        """Initialize.

        Details.
        """
        self.value = value
"#;

        let report = analyze_source_with_class_style(source, ClassDocstringStyle::Init);
        let diagnostic = single_rule(&report, "CLS205");
        assert_eq!(diagnostic.symbol.as_deref(), Some("__init__"));
        assert!(diagnostic.fix.as_ref().unwrap().edits[0].replacement.contains("Args:"));
        assert!(
            diagnostic.fix.as_ref().unwrap().edits[0]
                .replacement
                .contains("value (int):")
        );
    }

    #[test]
    fn emits_cls206_for_init_style_missing_init_raises_section() {
        let source = r#"class Example:
    def __init__(self, value: int) -> None:
        """Initialize.

        Args:
            value: The value.
        """
        raise ValueError("bad")
"#;

        let report = analyze_source_with_class_style(source, ClassDocstringStyle::Init);
        let diagnostic = single_rule(&report, "CLS206");
        assert_eq!(diagnostic.symbol.as_deref(), Some("__init__"));
        assert!(
            diagnostic.fix.as_ref().unwrap().edits[0]
                .replacement
                .contains("Raises:")
        );
        assert!(
            diagnostic.fix.as_ref().unwrap().edits[0]
                .replacement
                .contains("ValueError:")
        );
    }

    #[test]
    fn class_style_both_allows_class_or_init_sections() {
        let source = r#"class Example:
    """Class docs.

    Args:
        value: The value.
    """

    def __init__(self, value: int) -> None:
        """Initialize.

        Raises:
            ValueError: Bad value.
        """
        raise ValueError("bad")
"#;

        let report = analyze_source_with_class_style(source, ClassDocstringStyle::Both);

        assert!(
            report
                .diagnostics
                .iter()
                .all(|diagnostic| !diagnostic.rule.starts_with("CLS"))
        );
    }

    #[test]
    fn emits_prm001_for_missing_args_section() {
        let source = r#"def value(x: int, y: str) -> None:
    """Do something."""
    pass
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "PRM001");
        let fix = diagnostic.fix.as_ref().expect("PRM001 should append a stub section");
        assert_eq!(fix.applicability, Applicability::Unsafe);
        assert!(fix.edits[0].replacement.contains("Args:"));
        assert!(fix.edits[0].replacement.contains("x (int):"));
        assert!(fix.edits[0].replacement.contains("y (str):"));
    }

    #[test]
    fn emits_prm002_for_args_section_without_parameters() {
        let source = r#"def value() -> None:
    """Do something.

    Args:
        x: Not real.
    """
    pass
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "PRM002");
        assert_eq!(
            diagnostic.message,
            "Function has no parameters but docstring has Args/Parameters section."
        );
        assert_eq!(diagnostic.fix.as_ref().unwrap().applicability, Applicability::Safe);
    }

    #[test]
    fn emits_prm003_for_documented_self() {
        let source = r#"class Example:
    def method(self, x: int) -> None:
        """Do something.

        Args:
            self: The instance.
            x: The value.
        """
        pass
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "PRM003");
        assert_eq!(diagnostic.message, "Docstring should not document 'self'.");
        assert_eq!(diagnostic.fix.as_ref().unwrap().applicability, Applicability::Safe);
    }

    #[test]
    fn emits_prm004_for_missing_documented_parameter() {
        let source = r#"def value(x: int, y: str) -> None:
    """Do something.

    Args:
        x: The first value.
    """
    pass
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "PRM004");
        assert_eq!(diagnostic.message, "Missing parameter 'y' in docstring.");
        assert!(
            diagnostic.fix.as_ref().unwrap().edits[0]
                .replacement
                .contains("y (str):")
        );
    }

    #[test]
    fn emits_prm005_for_extra_documented_parameter() {
        let source = r#"def value(x: int) -> None:
    """Do something.

    Args:
        x: The first value.
        z: Extra.
    """
    pass
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "PRM005");
        assert_eq!(diagnostic.message, "Parameter 'z' not in function signature.");
        assert_eq!(diagnostic.fix.as_ref().unwrap().applicability, Applicability::Unsafe);
    }

    #[test]
    fn emits_prm006_for_wrong_parameter_order() {
        let source = r#"def value(x: int, y: str) -> None:
    """Do something.

    Args:
        y: The second value.
        x: The first value.
    """
    pass
"#;

        let report = analyze_source(source);
        let matches: Vec<&Diagnostic> = report
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.rule == "PRM006")
            .collect();
        assert_eq!(matches.len(), 2, "diagnostics: {:#?}", report.diagnostics);
        assert_eq!(
            matches[0].message,
            "Parameter 'y' is in the wrong order (expected 'x' at this position)."
        );
        assert_eq!(matches[0].fix.as_ref().unwrap().applicability, Applicability::Unsafe);
        assert!(matches[1].fix.is_none());
    }

    #[test]
    fn emits_prm007_for_duplicate_parameter() {
        let source = r#"def value(x: int) -> None:
    """Do something.

    Args:
        x: First.
        x: Duplicate.
    """
    pass
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "PRM007");
        assert_eq!(diagnostic.message, "Parameter 'x' is documented more than once.");
        assert_eq!(diagnostic.fix.as_ref().unwrap().applicability, Applicability::Unsafe);
    }

    #[test]
    fn emits_prm008_for_parameter_without_description() {
        let source = r#"def value(x: int) -> None:
    """Do something.

    Args:
        x:
    """
    pass
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "PRM008");
        assert_eq!(diagnostic.message, "Parameter 'x' has no description.");
        assert!(diagnostic.fix.is_none());
    }

    #[test]
    fn emits_prm009_for_missing_vararg_prefix() {
        let source = r#"def value(*args: int, **kwargs: str) -> None:
    """Do something.

    Args:
        args: Values.
        kwargs: Options.
    """
    pass
"#;

        let report = analyze_source(source);
        let matches: Vec<&Diagnostic> = report
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.rule == "PRM009")
            .collect();
        assert_eq!(matches.len(), 2, "diagnostics: {:#?}", report.diagnostics);
        assert_eq!(matches[0].fix.as_ref().unwrap().edits[0].replacement, "*args");
        assert_eq!(matches[1].fix.as_ref().unwrap().edits[0].replacement, "**kwargs");
    }

    #[test]
    fn emits_prm101_for_parameter_type_mismatch() {
        let source = r#"def value(x: int) -> None:
    """Do something.

    Args:
        x (str): The value.
    """
    pass
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "PRM101");
        assert_eq!(
            diagnostic.message,
            "Docstring type 'str' does not match type hint 'int' for parameter 'x'."
        );
        let fix = diagnostic.fix.as_ref().expect("PRM101 should replace docstring type");
        assert_eq!(fix.applicability, Applicability::Unsafe);
        assert_eq!(fix.edits[0].replacement, "int");
    }

    #[test]
    fn emits_prm102_when_parameter_type_is_missing_everywhere() {
        let source = r#"def value(x) -> None:
    """Do something.

    Args:
        x: The value.
    """
    pass
"#;

        let report = analyze_source(source);
        let diagnostic = single_rule(&report, "PRM102");
        assert_eq!(
            diagnostic.message,
            "Parameter 'x' has no type in docstring or signature."
        );
        assert!(diagnostic.fix.is_none());
    }

    #[test]
    fn emits_prm103_for_docstring_style_missing_docstring_type() {
        let source = r#"def value(x: int) -> None:
    """Do something.

    Args:
        x: The value.
    """
    pass
"#;

        let report = analyze_source_with_type_style(source, TypeAnnotationStyle::Docstring);
        let diagnostic = single_rule(&report, "PRM103");
        let fix = diagnostic.fix.as_ref().expect("PRM103 should insert docstring type");
        assert_eq!(fix.applicability, Applicability::Unsafe);
        assert_eq!(fix.edits[0].replacement, " (int)");
        assert!(report.diagnostics.iter().all(|diagnostic| diagnostic.rule != "PRM102"));
    }

    #[test]
    fn emits_prm104_for_signature_style_redundant_docstring_type() {
        let source = r#"def value(x: int) -> None:
    """Do something.

    Args:
        x (int): The value.
    """
    pass
"#;

        let report = analyze_source_with_type_style(source, TypeAnnotationStyle::Signature);
        let diagnostic = single_rule(&report, "PRM104");
        let fix = diagnostic.fix.as_ref().expect("PRM104 should delete docstring type");
        assert_eq!(fix.applicability, Applicability::Safe);
        assert_eq!(&source[fix.edits[0].range.start..fix.edits[0].range.end], " (int)");
    }

    #[test]
    fn emits_prm105_for_signature_style_missing_signature_type() {
        let source = r#"def value(x) -> None:
    """Do something.

    Args:
        x (int): The value.
    """
    pass
"#;

        let report = analyze_source_with_type_style(source, TypeAnnotationStyle::Signature);
        let diagnostic = single_rule(&report, "PRM105");
        assert_eq!(diagnostic.message, "Parameter 'x' has no type annotation in signature.");
        assert!(diagnostic.fix.is_none());
    }

    #[test]
    fn emits_prm106_for_docstring_style_signature_type() {
        let source = r#"def value(x: int) -> None:
    """Do something.

    Args:
        x (int): The value.
    """
    pass
"#;

        let report = analyze_source_with_type_style(source, TypeAnnotationStyle::Docstring);
        let diagnostic = single_rule(&report, "PRM106");
        assert_eq!(
            diagnostic.message,
            "Parameter 'x' has a type annotation in signature; types belong in the docstring."
        );
        assert!(diagnostic.fix.is_none());
    }

    #[test]
    fn emits_prm201_when_enabled_for_default_parameter_missing_optional() {
        let source = r#"def value(x: int = 0) -> None:
    """Do something.

    Args:
        x (int): The value.
    """
    pass
"#;

        let report = analyze_source_with_config(
            source,
            AnalysisConfig {
                type_annotation_style: None,
                class_docstring_style: None,
                allow_optional_shorthand: false,
                enable_prm201: true,
                enable_prm202: false,
            },
        );
        let diagnostic = single_rule(&report, "PRM201");
        assert_eq!(
            diagnostic.message,
            "Parameter 'x' has default value but docstring does not mention 'optional'."
        );
        let fix = diagnostic.fix.as_ref().expect("PRM201 should add optional");
        assert_eq!(fix.applicability, Applicability::Unsafe);
        assert_eq!(fix.edits[0].replacement, ", optional");
    }

    #[test]
    fn prm201_is_disabled_by_default() {
        let source = r#"def value(x: int = 0) -> None:
    """Do something.

    Args:
        x (int): The value.
    """
    pass
"#;

        let report = analyze_source(source);
        assert!(report.diagnostics.iter().all(|diagnostic| diagnostic.rule != "PRM201"));
    }

    #[test]
    fn prm201_accepts_optional_marker() {
        let source = r#"def value(x: int = 0) -> None:
    """Do something.

    Args:
        x (int, optional): The value.
    """
    pass
"#;

        let report = analyze_source(source);
        assert!(report.diagnostics.iter().all(|diagnostic| diagnostic.rule != "PRM201"));
    }

    #[test]
    fn emits_prm202_when_enabled_for_default_parameter_missing_default_mention() {
        let source = r#"def value(x: int = 42) -> None:
    """Do something.

    Args:
        x (int, optional): The value.
    """
    pass
"#;

        let report = analyze_source_with_config(
            source,
            AnalysisConfig {
                type_annotation_style: None,
                class_docstring_style: None,
                allow_optional_shorthand: false,
                enable_prm201: false,
                enable_prm202: true,
            },
        );
        let diagnostic = single_rule(&report, "PRM202");
        assert_eq!(
            diagnostic.message,
            "Parameter 'x' has default value but docstring does not mention 'default'."
        );
        let fix = diagnostic.fix.as_ref().expect("PRM202 should append default text");
        assert_eq!(fix.applicability, Applicability::Unsafe);
        assert_eq!(fix.edits[0].replacement, " Defaults to 42.");
    }

    #[test]
    fn prm202_is_disabled_by_default() {
        let source = r#"def value(x: int = 42) -> None:
    """Do something.

    Args:
        x (int, optional): The value.
    """
    pass
"#;

        let report = analyze_source(source);
        assert!(report.diagnostics.iter().all(|diagnostic| diagnostic.rule != "PRM202"));
    }

    fn analyze_source_with_type_style(source: &str, type_annotation_style: TypeAnnotationStyle) -> FileReport {
        analyze_source_with_config(
            source,
            AnalysisConfig {
                type_annotation_style: Some(type_annotation_style),
                class_docstring_style: None,
                allow_optional_shorthand: false,
                enable_prm201: false,
                enable_prm202: false,
            },
        )
    }

    fn analyze_source_with_class_style(source: &str, class_docstring_style: ClassDocstringStyle) -> FileReport {
        analyze_source_with_config(
            source,
            AnalysisConfig {
                type_annotation_style: None,
                class_docstring_style: Some(class_docstring_style),
                allow_optional_shorthand: false,
                enable_prm201: false,
                enable_prm202: false,
            },
        )
    }

    fn single_rule<'a>(report: &'a FileReport, rule: &str) -> &'a Diagnostic {
        let matches: Vec<&Diagnostic> = report
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.rule == rule)
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "expected exactly one {rule} diagnostic: {:#?}",
            report.diagnostics
        );
        matches[0]
    }
}
