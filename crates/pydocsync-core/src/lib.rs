//! Rust core for pydocsync.
//!
//! The rewrite pipeline starts with a lightweight Python source scan, then
//! parses each discovered docstring literal through `docstring-cst`.

use std::sync::Arc;

use docstring_cst::{DocstringStyle, Source, TextRange};
use pydocsync_scanner::{Item, ParameterRecord, RaiseRecord, summarize_python};

mod linter;
mod model;
mod registry;
mod rules;

pub use linter::Linter;
pub use model::{
    AnalysisConfig, Applicability, Diagnostic, DocstringHost, Edit, FileReport, Fix, HostKind, ParsedDocstring,
    RaisedException, Range, RuleFilter, SignatureParameter,
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
