//! Rust core for pydocfix.
//!
//! The rewrite pipeline starts with a lightweight Python source scan, then
//! parses each discovered docstring literal through `docstring-cst`.

use std::sync::Arc;

use docstring_cst::semantic::{BlockKind, SemanticBlock};
use docstring_cst::{DocstringStyle, Source, TextRange};
use pydocfix_scanner::{Item, ParameterRecord, RaiseRecord, summarize_python};

mod model;

pub use model::{
    AnalysisConfig, Applicability, ClassDocstringStyle, Diagnostic, DocstringHost, Edit, FileReport, Fix, HostKind,
    ParsedDocstring, RaisedException, Range, SignatureParameter, TypeAnnotationStyle,
};

/// Analyze Python source with the first Rust rewrite pipeline.
pub fn analyze_source(source: &str) -> FileReport {
    analyze_source_with_config(source, AnalysisConfig::default())
}

/// Analyze Python source with explicit rule configuration.
pub fn analyze_source_with_config(source: &str, config: AnalysisConfig) -> FileReport {
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
            diagnostics.extend(check_summary_rules(&source_buffer, &host, &semantic));
            diagnostics.extend(check_return_rules(&source_buffer, &host, &semantic, config));
            diagnostics.extend(check_yield_rules(&source_buffer, &host, &semantic, config));
            diagnostics.extend(check_raise_rules(&source_buffer, &host, &semantic));
            diagnostics.extend(check_doc_rules(&source_buffer, &host, &semantic));
            diagnostics.extend(check_class_rules(&source_buffer, &host, &semantic, config));
            diagnostics.extend(check_parameter_rules(&source_buffer, &host, &semantic, config));
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
        diagnostics,
    }
}

fn check_doc_rules(
    source: &Source,
    host: &DocstringHost,
    semantic: &docstring_cst::semantic::SemanticView,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if let Some(diagnostic) = check_doc001(source, host, semantic) {
        diagnostics.push(diagnostic);
    }
    diagnostics.extend(check_doc002(source, host, semantic));
    if let Some(diagnostic) = check_doc003(source, host, semantic) {
        diagnostics.push(diagnostic);
    }

    diagnostics
}

fn check_class_rules(
    source: &Source,
    host: &DocstringHost,
    semantic: &docstring_cst::semantic::SemanticView,
    config: AnalysisConfig,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if host.kind == HostKind::Function
        && host.name.as_deref() == Some("__init__")
        && host.parent_class_docstring_range.is_some()
        && config.class_docstring_style != Some(ClassDocstringStyle::Both)
    {
        diagnostics.push(Diagnostic {
            rule: "CLS001",
            message: "__init__ has its own docstring but the class also has a docstring.".to_string(),
            range: semantic
                .summary()
                .map(|summary| summary.entry_range.into())
                .unwrap_or(host.docstring_range),
            fix: None,
            symbol: host.name.clone(),
        });
    }

    for block in semantic.blocks() {
        match (host.kind, host.name.as_deref(), block.kind) {
            (HostKind::Class, _, BlockKind::Returns) => diagnostics.push(section_diagnostic(
                "CLS101",
                "Class docstring should not have a Returns section.",
                host,
                block,
                Applicability::Safe,
            )),
            (HostKind::Class, _, BlockKind::Yields) => diagnostics.push(section_diagnostic(
                "CLS102",
                "Class docstring should not have a Yields section.",
                host,
                block,
                Applicability::Safe,
            )),
            (HostKind::Function, Some("__init__"), BlockKind::Returns) => diagnostics.push(section_diagnostic(
                "CLS201",
                "__init__ docstring should not have a Returns section.",
                host,
                block,
                Applicability::Safe,
            )),
            (HostKind::Function, Some("__init__"), BlockKind::Yields) => diagnostics.push(section_diagnostic(
                "CLS202",
                "__init__ docstring should not have a Yields section.",
                host,
                block,
                Applicability::Safe,
            )),
            (HostKind::Class, _, BlockKind::Parameters)
                if config.class_docstring_style == Some(ClassDocstringStyle::Init) =>
            {
                diagnostics.push(section_diagnostic(
                    "CLS103",
                    "Class docstring should not have an Args/Parameters section when class_docstring_style is 'init'.",
                    host,
                    block,
                    Applicability::Unsafe,
                ));
            }
            (HostKind::Class, _, BlockKind::Raises)
                if config.class_docstring_style == Some(ClassDocstringStyle::Init) =>
            {
                diagnostics.push(section_diagnostic(
                    "CLS104",
                    "Class docstring should not have a Raises section when class_docstring_style is 'init'.",
                    host,
                    block,
                    Applicability::Unsafe,
                ));
            }
            (HostKind::Function, Some("__init__"), BlockKind::Parameters)
                if config.class_docstring_style == Some(ClassDocstringStyle::Class) =>
            {
                diagnostics.push(section_diagnostic(
                    "CLS203",
                    "__init__ docstring should not have an Args/Parameters section when class_docstring_style is 'class'.",
                    host,
                    block,
                    Applicability::Unsafe,
                ));
            }
            (HostKind::Function, Some("__init__"), BlockKind::Raises)
                if config.class_docstring_style == Some(ClassDocstringStyle::Class) =>
            {
                diagnostics.push(section_diagnostic(
                    "CLS204",
                    "__init__ docstring should not have a Raises section when class_docstring_style is 'class'.",
                    host,
                    block,
                    Applicability::Unsafe,
                ));
            }
            _ => {}
        }
    }

    if !is_short_plain_docstring(semantic) {
        let has_parameters = semantic
            .blocks()
            .iter()
            .any(|block| block.kind == BlockKind::Parameters);
        let has_raises = semantic.blocks().iter().any(|block| block.kind == BlockKind::Raises);

        if host.kind == HostKind::Class && config.class_docstring_style == Some(ClassDocstringStyle::Class) {
            if !has_parameters && !documentable_signature_parameters(host).is_empty() {
                diagnostics.push(missing_section_diagnostic(
                    "CLS105",
                    "Class docstring is missing an Args/Parameters section (class_docstring_style is 'class').",
                    source,
                    host,
                    semantic,
                    args_section_stub(source, host, semantic.style()),
                ));
            }
            if !has_raises && !host.raised_exceptions.is_empty() {
                diagnostics.push(missing_section_diagnostic(
                    "CLS106",
                    "Class docstring is missing a Raises section (class_docstring_style is 'class').",
                    source,
                    host,
                    semantic,
                    raises_section_stub(source, host, semantic.style()),
                ));
            }
        }

        if host.kind == HostKind::Function
            && host.name.as_deref() == Some("__init__")
            && config.class_docstring_style == Some(ClassDocstringStyle::Init)
        {
            if !has_parameters && !documentable_signature_parameters(host).is_empty() {
                diagnostics.push(missing_section_diagnostic(
                    "CLS205",
                    "__init__ docstring is missing an Args/Parameters section (class_docstring_style is 'init').",
                    source,
                    host,
                    semantic,
                    args_section_stub(source, host, semantic.style()),
                ));
            }
            if !has_raises && !host.raised_exceptions.is_empty() {
                diagnostics.push(missing_section_diagnostic(
                    "CLS206",
                    "__init__ docstring is missing a Raises section (class_docstring_style is 'init').",
                    source,
                    host,
                    semantic,
                    raises_section_stub(source, host, semantic.style()),
                ));
            }
        }
    }

    diagnostics
}

fn is_short_plain_docstring(semantic: &docstring_cst::semantic::SemanticView) -> bool {
    semantic.style() == DocstringStyle::Plain && semantic.extended_summary().is_none() && semantic.blocks().is_empty()
}

fn missing_section_diagnostic(
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

fn section_diagnostic(
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

fn check_parameter_rules(
    source: &Source,
    host: &DocstringHost,
    semantic: &docstring_cst::semantic::SemanticView,
    config: AnalysisConfig,
) -> Vec<Diagnostic> {
    if host.kind != HostKind::Function {
        return Vec::new();
    }

    let mut diagnostics = Vec::new();
    let parameter_block = semantic
        .blocks()
        .iter()
        .find(|block| block.kind == BlockKind::Parameters);
    let parameters = semantic.parameters();
    let signature_params = documentable_signature_parameters(host);

    if host.name.as_deref() != Some("__init__")
        && !signature_params.is_empty()
        && parameter_block.is_none()
        && parameters.is_empty()
    {
        let insert_offset = semantic
            .close_quote()
            .map(|quote| quote.entry_range.start())
            .unwrap_or(host.docstring_range.end);
        diagnostics.push(Diagnostic {
            rule: "PRM001",
            message: "Missing Args/Parameters section in docstring.".to_string(),
            range: semantic
                .summary()
                .map(|summary| summary.entry_range.into())
                .unwrap_or(host.docstring_range),
            fix: Some(Fix {
                edits: vec![Edit::insert(
                    insert_offset,
                    args_section_stub(source, host, semantic.style()),
                )],
                applicability: Applicability::Unsafe,
            }),
            symbol: host.name.clone(),
        });
    }

    if signature_params.is_empty()
        && let Some(block) = parameter_block
    {
        diagnostics.push(Diagnostic {
            rule: "PRM002",
            message: "Function has no parameters but docstring has Args/Parameters section.".to_string(),
            range: block.name_range.into(),
            fix: Some(Fix {
                edits: vec![Edit {
                    range: block.entry_range.into(),
                    replacement: String::new(),
                }],
                applicability: Applicability::Safe,
            }),
            symbol: host.name.clone(),
        });
    }

    let documented = documented_parameters(source, semantic);
    for documented_param in &documented {
        if matches!(documented_param.name.as_str(), "self" | "cls") {
            diagnostics.push(Diagnostic {
                rule: "PRM003",
                message: format!("Docstring should not document '{}'.", documented_param.name),
                range: documented_param.name_range.into(),
                fix: Some(Fix {
                    edits: vec![Edit {
                        range: documented_param.entry_range.into(),
                        replacement: String::new(),
                    }],
                    applicability: Applicability::Safe,
                }),
                symbol: host.name.clone(),
            });
        }
    }

    if let Some(block) = parameter_block
        && !documented.is_empty()
    {
        for signature_param in signature_params {
            if documented
                .iter()
                .any(|documented_param| bare_parameter_name(&documented_param.name) == signature_param.bare_name)
            {
                continue;
            }
            diagnostics.push(Diagnostic {
                rule: "PRM004",
                message: format!("Missing parameter '{}' in docstring.", signature_param.name),
                range: block.name_range.into(),
                fix: Some(Fix {
                    edits: vec![Edit::insert(
                        block.entry_range.end(),
                        parameter_entry_append_text(source, block.name_range, semantic.style(), signature_param),
                    )],
                    applicability: Applicability::Unsafe,
                }),
                symbol: host.name.clone(),
            });
        }
    }

    let signature_names = all_signature_bare_names(host);
    for documented_param in &documented {
        let bare_name = bare_parameter_name(&documented_param.name);
        if !signature_names.iter().any(|name| *name == bare_name) {
            diagnostics.push(Diagnostic {
                rule: "PRM005",
                message: format!("Parameter '{}' not in function signature.", documented_param.name),
                range: documented_param.name_range.into(),
                fix: Some(Fix {
                    edits: vec![Edit {
                        range: documented_param.entry_range.into(),
                        replacement: String::new(),
                    }],
                    applicability: Applicability::Unsafe,
                }),
                symbol: host.name.clone(),
            });
        }
    }

    diagnostics.extend(check_prm006(source, host, &documented));
    diagnostics.extend(check_prm007(host, &documented));
    diagnostics.extend(check_prm008(source, host, &documented));
    diagnostics.extend(check_prm009(host, &documented));
    diagnostics.extend(check_parameter_default_rules(source, host, config, &documented));
    diagnostics.extend(check_parameter_type_rules(
        source,
        host,
        semantic.style(),
        config,
        &documented,
    ));

    diagnostics
}

fn check_parameter_default_rules(
    source: &Source,
    host: &DocstringHost,
    config: AnalysisConfig,
    documented: &[DocumentedParameter],
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for documented_param in documented {
        let bare_name = bare_parameter_name(&documented_param.name);
        let Some(signature_param) = host
            .signature_parameters
            .iter()
            .find(|param| param.bare_name == bare_name && !param.is_implicit_receiver)
        else {
            continue;
        };
        let Some(default_value) = signature_param.default_value.as_deref() else {
            continue;
        };

        if documented_param.type_range.is_some() && documented_param.optional_range.is_none() {
            diagnostics.push(Diagnostic {
                rule: "PRM201",
                message: format!(
                    "Parameter '{}' has default value but docstring does not mention 'optional'.",
                    documented_param.name
                ),
                range: documented_param.name_range.into(),
                fix: documented_param.type_range.map(|type_range| Fix {
                    edits: vec![Edit::insert(type_range.end(), ", optional")],
                    applicability: Applicability::Unsafe,
                }),
                symbol: host.name.clone(),
            });
        }

        if config.enable_prm202 && !documented_parameter_mentions_default(source, documented_param) {
            diagnostics.push(Diagnostic {
                rule: "PRM202",
                message: format!(
                    "Parameter '{}' has default value but docstring does not mention 'default'.",
                    documented_param.name
                ),
                range: documented_param.name_range.into(),
                fix: documented_param.description_range.and_then(|description_range| {
                    let description = source.slice(description_range)?.trim_end();
                    let suffix = if description.ends_with('.') { "" } else { "." };
                    Some(Fix {
                        edits: vec![Edit::insert(
                            description_range.end(),
                            format!("{suffix} Defaults to {default_value}."),
                        )],
                        applicability: Applicability::Unsafe,
                    })
                }),
                symbol: host.name.clone(),
            });
        }
    }
    diagnostics
}

fn check_parameter_type_rules(
    source: &Source,
    host: &DocstringHost,
    style: DocstringStyle,
    config: AnalysisConfig,
    documented: &[DocumentedParameter],
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for documented_param in documented {
        let bare_name = bare_parameter_name(&documented_param.name);
        let Some(signature_param) = host
            .signature_parameters
            .iter()
            .find(|param| param.bare_name == bare_name && !param.is_implicit_receiver)
        else {
            continue;
        };
        let doc_type = documented_param
            .type_range
            .and_then(|range| source.slice(range))
            .map(str::trim)
            .filter(|text| !text.is_empty());
        let signature_type = signature_param.annotation.as_deref();

        match (doc_type, signature_type) {
            (Some(doc_type), Some(signature_type)) if !types_match(doc_type, signature_type) => {
                let type_range = documented_param.type_range.unwrap();
                diagnostics.push(Diagnostic {
                    rule: "PRM101",
                    message: format!(
                        "Docstring type '{doc_type}' does not match type hint '{signature_type}' for parameter '{}'.",
                        documented_param.name
                    ),
                    range: type_range.into(),
                    fix: Some(Fix {
                        edits: vec![Edit {
                            range: type_range.into(),
                            replacement: signature_type.to_string(),
                        }],
                        applicability: Applicability::Unsafe,
                    }),
                    symbol: host.name.clone(),
                });
            }
            (None, None) if config.type_annotation_style.is_none() => diagnostics.push(Diagnostic {
                rule: "PRM102",
                message: format!(
                    "Parameter '{}' has no type in docstring or signature.",
                    documented_param.name
                ),
                range: documented_param.name_range.into(),
                fix: None,
                symbol: host.name.clone(),
            }),
            _ => {}
        }

        if matches!(
            config.type_annotation_style,
            Some(TypeAnnotationStyle::Docstring | TypeAnnotationStyle::Both)
        ) && doc_type.is_none()
        {
            diagnostics.push(Diagnostic {
                rule: "PRM103",
                message: format!("Parameter '{}' has no type in docstring.", documented_param.name),
                range: documented_param.name_range.into(),
                fix: signature_type.map(|signature_type| Fix {
                    edits: vec![Edit::insert(
                        documented_param.name_range.end(),
                        parameter_type_insert_text(style, signature_type),
                    )],
                    applicability: Applicability::Unsafe,
                }),
                symbol: host.name.clone(),
            });
        }

        if matches!(config.type_annotation_style, Some(TypeAnnotationStyle::Signature))
            && doc_type.is_some()
            && signature_type.is_some()
        {
            let type_range = documented_param.type_range.unwrap();
            diagnostics.push(Diagnostic {
                rule: "PRM104",
                message: format!("Parameter '{}' has redundant type in docstring.", documented_param.name),
                range: type_range.into(),
                fix: Some(Fix {
                    edits: vec![Edit {
                        range: parameter_type_delete_range(source, style, documented_param, type_range),
                        replacement: String::new(),
                    }],
                    applicability: Applicability::Safe,
                }),
                symbol: host.name.clone(),
            });
        }

        if matches!(
            config.type_annotation_style,
            Some(TypeAnnotationStyle::Signature | TypeAnnotationStyle::Both)
        ) && signature_type.is_none()
        {
            diagnostics.push(Diagnostic {
                rule: "PRM105",
                message: format!(
                    "Parameter '{}' has no type annotation in signature.",
                    documented_param.name
                ),
                range: documented_param.name_range.into(),
                fix: None,
                symbol: host.name.clone(),
            });
        }

        if matches!(config.type_annotation_style, Some(TypeAnnotationStyle::Docstring)) && signature_type.is_some() {
            diagnostics.push(Diagnostic {
                rule: "PRM106",
                message: format!(
                    "Parameter '{}' has a type annotation in signature; types belong in the docstring.",
                    documented_param.name
                ),
                range: documented_param.name_range.into(),
                fix: None,
                symbol: host.name.clone(),
            });
        }
    }
    diagnostics
}

fn check_prm006(source: &Source, host: &DocstringHost, documented: &[DocumentedParameter]) -> Vec<Diagnostic> {
    let signature_order: Vec<&str> = documentable_signature_parameters(host)
        .into_iter()
        .map(|param| param.bare_name.as_str())
        .collect();
    if signature_order.is_empty() || documented.len() < 2 {
        return Vec::new();
    }
    let documented_in_signature: Vec<&DocumentedParameter> = documented
        .iter()
        .filter(|param| signature_order.contains(&bare_parameter_name(&param.name)))
        .collect();
    let documented_names: Vec<&str> = documented_in_signature
        .iter()
        .map(|param| bare_parameter_name(&param.name))
        .collect();
    let expected: Vec<&str> = signature_order
        .iter()
        .copied()
        .filter(|name| documented_names.contains(name))
        .collect();
    if documented_names == expected {
        return Vec::new();
    }

    let fix = reorder_parameters_fix(source, documented, &signature_order);
    let mut diagnostics = Vec::new();
    for (index, (documented_param, expected_name)) in documented_in_signature.iter().zip(expected.iter()).enumerate() {
        let doc_name = bare_parameter_name(&documented_param.name);
        if doc_name == *expected_name {
            continue;
        }
        diagnostics.push(Diagnostic {
            rule: "PRM006",
            message: format!(
                "Parameter '{doc_name}' is in the wrong order (expected '{expected_name}' at this position)."
            ),
            range: documented_param.name_range.into(),
            fix: (index == 0).then(|| fix.clone()),
            symbol: host.name.clone(),
        });
    }
    diagnostics
}

fn check_prm007(host: &DocstringHost, documented: &[DocumentedParameter]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen = Vec::new();
    for documented_param in documented {
        if seen.contains(&documented_param.name.as_str()) {
            diagnostics.push(Diagnostic {
                rule: "PRM007",
                message: format!("Parameter '{}' is documented more than once.", documented_param.name),
                range: documented_param.name_range.into(),
                fix: Some(Fix {
                    edits: vec![Edit {
                        range: documented_param.entry_range.into(),
                        replacement: String::new(),
                    }],
                    applicability: Applicability::Unsafe,
                }),
                symbol: host.name.clone(),
            });
        } else {
            seen.push(documented_param.name.as_str());
        }
    }
    diagnostics
}

fn check_prm008(source: &Source, host: &DocstringHost, documented: &[DocumentedParameter]) -> Vec<Diagnostic> {
    documented
        .iter()
        .filter(|param| {
            param
                .description_range
                .and_then(|range| source.slice(range))
                .is_none_or(|description| description.trim().is_empty())
        })
        .map(|param| Diagnostic {
            rule: "PRM008",
            message: format!("Parameter '{}' has no description.", param.name),
            range: param.name_range.into(),
            fix: None,
            symbol: host.name.clone(),
        })
        .collect()
}

fn check_prm009(host: &DocstringHost, documented: &[DocumentedParameter]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for documented_param in documented {
        if documented_param.name.starts_with('*') {
            continue;
        }
        let Some(signature_param) = host
            .signature_parameters
            .iter()
            .find(|param| param.bare_name == documented_param.name && (param.is_vararg || param.is_kwarg))
        else {
            continue;
        };
        diagnostics.push(Diagnostic {
            rule: "PRM009",
            message: format!(
                "Docstring parameter '{}' should be '{}'.",
                documented_param.name, signature_param.name
            ),
            range: documented_param.name_range.into(),
            fix: Some(Fix {
                edits: vec![Edit {
                    range: documented_param.name_range.into(),
                    replacement: signature_param.name.clone(),
                }],
                applicability: Applicability::Safe,
            }),
            symbol: host.name.clone(),
        });
    }
    diagnostics
}

#[derive(Clone, Debug)]
struct DocumentedParameter {
    name: String,
    entry_range: TextRange,
    name_range: TextRange,
    type_range: Option<TextRange>,
    description_range: Option<TextRange>,
    optional_range: Option<TextRange>,
    default_value_range: Option<TextRange>,
}

fn documented_parameters(
    source: &Source,
    semantic: &docstring_cst::semantic::SemanticView,
) -> Vec<DocumentedParameter> {
    semantic
        .parameters()
        .iter()
        .filter_map(|param| {
            let name_range = param.name_range?;
            let name = source.slice(name_range)?.trim().to_string();
            Some(DocumentedParameter {
                name,
                entry_range: param.entry_range,
                name_range,
                type_range: param.type_range,
                description_range: param.description_range,
                optional_range: param.optional_range,
                default_value_range: param.default_value_range,
            })
        })
        .collect()
}

fn documented_parameter_mentions_default(source: &Source, documented_param: &DocumentedParameter) -> bool {
    if documented_param.default_value_range.is_some() {
        return true;
    }
    documented_param
        .description_range
        .and_then(|range| source.slice(range))
        .is_some_and(contains_default_word)
}

fn contains_default_word(text: &str) -> bool {
    let mut word = String::new();
    for character in text.chars().chain(std::iter::once(' ')) {
        if character.is_ascii_alphabetic() {
            word.push(character.to_ascii_lowercase());
        } else {
            if matches!(word.as_str(), "default" | "defaults") {
                return true;
            }
            word.clear();
        }
    }
    false
}

fn parameter_type_insert_text(style: DocstringStyle, signature_type: &str) -> String {
    match style {
        DocstringStyle::Numpy => format!(" : {signature_type}"),
        _ => format!(" ({signature_type})"),
    }
}

fn parameter_type_delete_range(
    source: &Source,
    style: DocstringStyle,
    documented_param: &DocumentedParameter,
    type_range: TextRange,
) -> Range {
    let bytes = source.source().as_bytes();
    match style {
        DocstringStyle::Numpy => {
            let mut start = type_range.start();
            while start > documented_param.name_range.end() && matches!(bytes.get(start - 1), Some(b' ' | b'\t' | b':'))
            {
                start -= 1;
            }
            Range {
                start,
                end: type_range.end(),
            }
        }
        _ => {
            let mut start = type_range.start();
            let mut end = type_range.end();
            if start > documented_param.name_range.end() && bytes.get(start - 1) == Some(&b'(') {
                start -= 1;
                while start > documented_param.name_range.end() && matches!(bytes.get(start - 1), Some(b' ' | b'\t')) {
                    start -= 1;
                }
            }
            if bytes.get(end) == Some(&b')') {
                end += 1;
            }
            Range { start, end }
        }
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

fn documentable_signature_parameters(host: &DocstringHost) -> Vec<&SignatureParameter> {
    host.signature_parameters
        .iter()
        .filter(|param| !param.is_implicit_receiver)
        .collect()
}

fn all_signature_bare_names(host: &DocstringHost) -> Vec<&str> {
    host.signature_parameters
        .iter()
        .map(|param| param.bare_name.as_str())
        .collect()
}

fn bare_parameter_name(name: &str) -> &str {
    name.trim_start_matches('*')
}

fn args_section_stub(source: &Source, host: &DocstringHost, style: DocstringStyle) -> String {
    let indent = line_indent_before(source.source(), host.docstring_range.start);
    let params = documentable_signature_parameters(host);
    match style {
        DocstringStyle::Numpy => {
            let mut stub = format!("\n\n{indent}Parameters\n{indent}----------");
            for param in params {
                if let Some(annotation) = &param.annotation {
                    stub.push_str(&format!("\n{indent}{} : {annotation}", param.name));
                } else {
                    stub.push_str(&format!("\n{indent}{}", param.name));
                }
            }
            stub.push_str(&format!("\n{indent}"));
            stub
        }
        _ => {
            let mut stub = format!("\n\n{indent}Args:");
            for param in params {
                if let Some(annotation) = &param.annotation {
                    stub.push_str(&format!("\n{indent}    {} ({annotation}):", param.name));
                } else {
                    stub.push_str(&format!("\n{indent}    {}:", param.name));
                }
            }
            stub.push_str(&format!("\n{indent}"));
            stub
        }
    }
}

fn parameter_entry_append_text(
    source: &Source,
    header_range: TextRange,
    style: DocstringStyle,
    param: &SignatureParameter,
) -> String {
    let header_indent = line_indent_before(source.source(), header_range.start());
    match style {
        DocstringStyle::Numpy => {
            if let Some(annotation) = &param.annotation {
                format!("\n{header_indent}{} : {annotation}", param.name)
            } else {
                format!("\n{header_indent}{}", param.name)
            }
        }
        _ => {
            if let Some(annotation) = &param.annotation {
                format!("\n{header_indent}    {} ({annotation}):", param.name)
            } else {
                format!("\n{header_indent}    {}:", param.name)
            }
        }
    }
}

fn reorder_parameters_fix(source: &Source, documented: &[DocumentedParameter], signature_order: &[&str]) -> Fix {
    let mut sorted = documented.to_vec();
    sorted.sort_by_key(|param| {
        signature_order
            .iter()
            .position(|name| *name == bare_parameter_name(&param.name))
            .unwrap_or(signature_order.len())
    });
    let start = documented.first().map(|param| param.entry_range.start()).unwrap_or(0);
    let end = documented.last().map(|param| param.entry_range.end()).unwrap_or(start);
    let replacement = sorted
        .iter()
        .filter_map(|param| source.slice(param.entry_range))
        .collect::<String>();
    Fix {
        edits: vec![Edit {
            range: Range { start, end },
            replacement,
        }],
        applicability: Applicability::Unsafe,
    }
}

fn check_doc001(
    source: &Source,
    host: &DocstringHost,
    semantic: &docstring_cst::semantic::SemanticView,
) -> Option<Diagnostic> {
    if !matches!(host.kind, HostKind::Function | HostKind::Class) {
        return None;
    }
    if !matches!(semantic.style(), DocstringStyle::Google | DocstringStyle::Numpy) {
        return None;
    }

    let blocks = semantic.blocks();
    if blocks.len() < 2 {
        return None;
    }

    let sorted_indices = sorted_block_indices(blocks);
    if sorted_indices.iter().copied().eq(0..blocks.len()) {
        return None;
    }

    let first_wrong_index = sorted_indices
        .iter()
        .enumerate()
        .find_map(|(index, sorted_index)| (index != *sorted_index).then_some(index))
        .unwrap_or(0);
    let range = blocks[first_wrong_index].entry_range.into();

    let start = blocks.first()?.entry_range.start();
    let end = blocks.last()?.entry_range.end();
    let original = source.source().get(start..end)?;
    let mut replacement = String::new();
    for (position, block_index) in sorted_indices.iter().enumerate() {
        let block = blocks[*block_index];
        replacement.push_str(source.slice(block.entry_range)?);
        if position + 1 < sorted_indices.len() {
            let gap_start = blocks[position].entry_range.end();
            let gap_end = blocks[position + 1].entry_range.start();
            replacement.push_str(source.source().get(gap_start..gap_end).unwrap_or(""));
        }
    }
    if replacement == original {
        return None;
    }

    Some(Diagnostic {
        rule: "DOC001",
        message: "Docstring sections are not in canonical order.".to_string(),
        range,
        fix: Some(Fix {
            edits: vec![Edit {
                range: Range { start, end },
                replacement,
            }],
            applicability: Applicability::Unsafe,
        }),
        symbol: host.name.clone(),
    })
}

fn sorted_block_indices(blocks: &[SemanticBlock]) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..blocks.len()).collect();
    indices.sort_by_key(|index| (block_order(blocks[*index].kind), *index));
    indices
}

fn block_order(kind: BlockKind) -> usize {
    match kind {
        BlockKind::Parameters => 0,
        BlockKind::Receives => 1,
        BlockKind::Returns => 2,
        BlockKind::Yields => 3,
        BlockKind::Raises => 4,
        BlockKind::Warns => 5,
        BlockKind::Attributes => 6,
        BlockKind::Methods => 7,
        BlockKind::Notes => 8,
        BlockKind::References => 9,
        BlockKind::Examples => 10,
        BlockKind::SeeAlso => 11,
        BlockKind::Other => 12,
        _ => 12,
    }
}

fn check_doc002(
    source: &Source,
    host: &DocstringHost,
    semantic: &docstring_cst::semantic::SemanticView,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let is_numpy = semantic.style() == DocstringStyle::Numpy;
    for entry_range in doc_entry_ranges(semantic) {
        let Some(block) = semantic.blocks().iter().find(|block| {
            block.entry_range.start() <= entry_range.start() && entry_range.start() < block.entry_range.end()
        }) else {
            continue;
        };
        let line_start = source.source()[..entry_range.start()]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        let entry_start = first_non_ws_on_line(source.source(), entry_range.start(), entry_range.end());
        let actual_indent = entry_start - line_start;
        let section_indent = block.header_indent_range.end() - block.header_indent_range.start();
        let expected_indent = if is_numpy { section_indent } else { section_indent + 4 };
        if actual_indent == expected_indent {
            continue;
        }
        diagnostics.push(Diagnostic {
            rule: "DOC002",
            message: format!("Expected {expected_indent}-space indentation, found {actual_indent}."),
            range: TextRange::new(entry_start, entry_range.end()).into(),
            fix: Some(Fix {
                edits: vec![Edit {
                    range: Range {
                        start: line_start,
                        end: entry_start,
                    },
                    replacement: " ".repeat(expected_indent),
                }],
                applicability: Applicability::Safe,
            }),
            symbol: host.name.clone(),
        });
    }
    diagnostics
}

fn first_non_ws_on_line(source: &str, start: usize, end: usize) -> usize {
    let bytes = source.as_bytes();
    let mut offset = start;
    while offset < end && matches!(bytes[offset], b' ' | b'\t') {
        offset += 1;
    }
    offset
}

fn doc_entry_ranges(semantic: &docstring_cst::semantic::SemanticView) -> Vec<TextRange> {
    let mut ranges = Vec::new();
    ranges.extend(semantic.parameters().iter().map(|entry| entry.entry_range));
    ranges.extend(semantic.returns().iter().map(|entry| entry.entry_range));
    ranges.extend(semantic.yields().iter().map(|entry| entry.entry_range));
    ranges.extend(semantic.raises().iter().map(|entry| entry.entry_range));
    ranges.extend(semantic.warns().iter().map(|entry| entry.entry_range));
    ranges.extend(semantic.attributes().iter().map(|entry| entry.entry_range));
    ranges.extend(semantic.methods().iter().map(|entry| entry.entry_range));
    ranges.sort_by_key(|range| range.start());
    ranges.dedup();
    ranges
}

fn check_doc003(
    source: &Source,
    host: &DocstringHost,
    semantic: &docstring_cst::semantic::SemanticView,
) -> Option<Diagnostic> {
    if !matches!(host.kind, HostKind::Module | HostKind::Class | HostKind::Function) {
        return None;
    }
    if semantic.style() != DocstringStyle::Plain {
        return None;
    }
    if semantic.extended_summary().is_some() || !semantic.blocks().is_empty() {
        return None;
    }
    let summary = semantic.summary()?;
    let open_quote = semantic.open_quote()?;
    let close_quote = semantic.close_quote()?;
    let body = source
        .source()
        .get(open_quote.entry_range.end()..close_quote.entry_range.start())?;
    if !body.contains('\n') {
        return None;
    }
    let non_empty_lines = body.lines().filter(|line| !line.trim().is_empty()).count();
    if non_empty_lines != 1 {
        return None;
    }
    let summary_text = source.slice(summary.entry_range)?.trim();
    if summary_text.is_empty() {
        return None;
    }
    let open_text = source.slice(open_quote.entry_range)?;
    let close_text = source.slice(close_quote.entry_range)?;
    Some(Diagnostic {
        rule: "DOC003",
        message: "One-line docstring should be written on a single line.".to_string(),
        range: summary.entry_range.into(),
        fix: Some(Fix {
            edits: vec![Edit {
                range: host.docstring_range,
                replacement: format!("{open_text}{summary_text}{close_text}"),
            }],
            applicability: Applicability::Safe,
        }),
        symbol: host.name.clone(),
    })
}

fn check_raise_rules(
    source: &Source,
    host: &DocstringHost,
    semantic: &docstring_cst::semantic::SemanticView,
) -> Vec<Diagnostic> {
    if host.kind != HostKind::Function {
        return Vec::new();
    }

    let mut diagnostics = Vec::new();
    let raises_block = semantic
        .blocks()
        .iter()
        .find(|block| block.kind == docstring_cst::semantic::BlockKind::Raises);
    let raises = semantic.raises();

    if host.name.as_deref() != Some("__init__")
        && !host.raised_exceptions.is_empty()
        && raises_block.is_none()
        && raises.is_empty()
    {
        let insert_offset = semantic
            .close_quote()
            .map(|quote| quote.entry_range.start())
            .unwrap_or(host.docstring_range.end);
        diagnostics.push(Diagnostic {
            rule: "RIS001",
            message: "Missing Raises section in docstring.".to_string(),
            range: semantic
                .summary()
                .map(|summary| summary.entry_range.into())
                .unwrap_or(host.docstring_range),
            fix: Some(Fix {
                edits: vec![Edit::insert(
                    insert_offset,
                    raises_section_stub(source, host, semantic.style()),
                )],
                applicability: Applicability::Unsafe,
            }),
            symbol: host.name.clone(),
        });
    }

    if host.raised_exceptions.is_empty()
        && let Some(block) = raises_block
    {
        diagnostics.push(Diagnostic {
            rule: "RIS002",
            message: "Unnecessary Raises section in docstring.".to_string(),
            range: block.name_range.into(),
            fix: Some(Fix {
                edits: vec![Edit {
                    range: block.entry_range.into(),
                    replacement: String::new(),
                }],
                applicability: Applicability::Safe,
            }),
            symbol: host.name.clone(),
        });
    }

    for raise_entry in raises {
        let has_description = raise_entry
            .description_range
            .and_then(|range| source.slice(range))
            .is_some_and(|text| !text.trim().is_empty());
        if !has_description {
            diagnostics.push(Diagnostic {
                rule: "RIS003",
                message: "Raises entry has no description.".to_string(),
                range: raise_entry
                    .exception_range
                    .or(raise_entry.description_range)
                    .unwrap_or(raise_entry.entry_range)
                    .into(),
                fix: None,
                symbol: host.name.clone(),
            });
        }
    }

    if let Some(block) = raises_block {
        let documented_names: Vec<&str> = raises
            .iter()
            .filter_map(|entry| entry.exception_range.and_then(|range| source.slice(range)))
            .map(bare_exception_name)
            .collect();

        for raised_exception in unique_raised_exception_names(host) {
            if documented_names
                .iter()
                .any(|documented| *documented == raised_exception)
            {
                continue;
            }
            diagnostics.push(Diagnostic {
                rule: "RIS004",
                message: format!("Raised exception '{raised_exception}' not documented in Raises section."),
                range: block.name_range.into(),
                fix: Some(Fix {
                    edits: vec![Edit::insert(
                        block.entry_range.end(),
                        raises_entry_append_text(source, block.name_range, semantic.style(), raised_exception),
                    )],
                    applicability: Applicability::Unsafe,
                }),
                symbol: host.name.clone(),
            });
        }
    }

    let raised_names = unique_raised_exception_names(host);
    for raise_entry in raises {
        let Some(exception_range) = raise_entry.exception_range else {
            continue;
        };
        let Some(documented_name) = source.slice(exception_range) else {
            continue;
        };
        let bare_documented_name = bare_exception_name(documented_name);
        if raised_names
            .iter()
            .any(|raised_name| *raised_name == bare_documented_name)
        {
            continue;
        }
        diagnostics.push(Diagnostic {
            rule: "RIS005",
            message: format!("Raises entry '{documented_name}' not raised in function body."),
            range: exception_range.into(),
            fix: Some(Fix {
                edits: vec![Edit {
                    range: raise_entry.entry_range.into(),
                    replacement: String::new(),
                }],
                applicability: Applicability::Unsafe,
            }),
            symbol: host.name.clone(),
        });
    }

    diagnostics
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

fn unique_raised_exception_names(host: &DocstringHost) -> Vec<&str> {
    let mut names = Vec::new();
    for exception in &host.raised_exceptions {
        let name = bare_exception_name(&exception.name);
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

fn bare_exception_name(name: &str) -> &str {
    name.trim().rsplit('.').next().unwrap_or(name.trim())
}

fn raises_section_stub(source: &Source, host: &DocstringHost, style: DocstringStyle) -> String {
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

fn raises_entry_append_text(
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

fn check_return_rules(
    source: &Source,
    host: &DocstringHost,
    semantic: &docstring_cst::semantic::SemanticView,
    config: AnalysisConfig,
) -> Vec<Diagnostic> {
    if host.kind != HostKind::Function {
        return Vec::new();
    }

    let mut diagnostics = Vec::new();
    let returns_block = semantic
        .blocks()
        .iter()
        .find(|block| block.kind == docstring_cst::semantic::BlockKind::Returns);
    let returns = semantic.returns();

    if !host.has_yield
        && returns_block.is_none()
        && returns.is_empty()
        && let Some(return_annotation) = meaningful_return_annotation(source, host)
    {
        let insert_offset = semantic
            .close_quote()
            .map(|quote| quote.entry_range.start())
            .unwrap_or(host.docstring_range.end);
        diagnostics.push(Diagnostic {
            rule: "RTN001",
            message: "Missing Returns section in docstring.".to_string(),
            range: semantic
                .summary()
                .map(|summary| summary.entry_range.into())
                .unwrap_or(host.docstring_range),
            fix: Some(Fix {
                edits: vec![Edit::insert(
                    insert_offset,
                    returns_section_stub(source, host, semantic.style(), return_annotation),
                )],
                applicability: Applicability::Unsafe,
            }),
            symbol: host.name.clone(),
        });
    }

    if !host.has_return_value
        && let Some(block) = returns_block
    {
        diagnostics.push(Diagnostic {
            rule: "RTN002",
            message: "Unnecessary Returns section in docstring.".to_string(),
            range: block.name_range.into(),
            fix: Some(Fix {
                edits: vec![Edit {
                    range: block.entry_range.into(),
                    replacement: String::new(),
                }],
                applicability: Applicability::Safe,
            }),
            symbol: host.name.clone(),
        });
    }

    for return_entry in returns {
        let has_description = return_entry
            .description_range
            .and_then(|range| source.slice(range))
            .is_some_and(|text| !text.trim().is_empty());
        if !has_description {
            diagnostics.push(Diagnostic {
                rule: "RTN003",
                message: "Returns section has no description.".to_string(),
                range: return_entry
                    .type_range
                    .or(return_entry.description_range)
                    .unwrap_or(return_entry.entry_range)
                    .into(),
                fix: None,
                symbol: host.name.clone(),
            });
        }
    }

    let signature_type = return_annotation(source, host);
    for return_entry in returns {
        let doc_type = return_entry
            .type_range
            .and_then(|range| source.slice(range))
            .map(str::trim)
            .filter(|text| !text.is_empty());
        match (doc_type, signature_type) {
            (Some(doc_type), Some(signature_type)) if !types_match(doc_type, signature_type) => {
                diagnostics.push(Diagnostic {
                    rule: "RTN101",
                    message: format!("Docstring return type '{doc_type}' does not match type hint '{signature_type}'."),
                    range: return_entry.type_range.unwrap().into(),
                    fix: Some(Fix {
                        edits: vec![Edit {
                            range: return_entry.type_range.unwrap().into(),
                            replacement: signature_type.to_string(),
                        }],
                        applicability: Applicability::Unsafe,
                    }),
                    symbol: host.name.clone(),
                });
            }
            (None, None) if config.type_annotation_style.is_none() => {
                diagnostics.push(Diagnostic {
                    rule: "RTN102",
                    message: "Return type not in docstring or signature.".to_string(),
                    range: return_entry.entry_range.into(),
                    fix: None,
                    symbol: host.name.clone(),
                });
            }
            _ => {}
        }

        if matches!(
            config.type_annotation_style,
            Some(TypeAnnotationStyle::Docstring | TypeAnnotationStyle::Both)
        ) && doc_type.is_none()
        {
            diagnostics.push(Diagnostic {
                rule: "RTN103",
                message: "Return has no type in docstring.".to_string(),
                range: return_entry.entry_range.into(),
                fix: signature_type.map(|signature_type| Fix {
                    edits: vec![Edit::insert(
                        return_entry.entry_range.start(),
                        return_type_insert_text(semantic.style(), signature_type),
                    )],
                    applicability: Applicability::Unsafe,
                }),
                symbol: host.name.clone(),
            });
        }

        if matches!(config.type_annotation_style, Some(TypeAnnotationStyle::Signature))
            && doc_type.is_some()
            && signature_type.is_some()
        {
            let type_range = return_entry.type_range.unwrap();
            diagnostics.push(Diagnostic {
                rule: "RTN104",
                message: "Redundant return type in docstring; type annotation exists in signature.".to_string(),
                range: type_range.into(),
                fix: Some(Fix {
                    edits: vec![Edit {
                        range: redundant_type_delete_range(source, semantic.style(), type_range),
                        replacement: String::new(),
                    }],
                    applicability: Applicability::Safe,
                }),
                symbol: host.name.clone(),
            });
        }

        if matches!(
            config.type_annotation_style,
            Some(TypeAnnotationStyle::Signature | TypeAnnotationStyle::Both)
        ) && signature_type.is_none()
        {
            diagnostics.push(Diagnostic {
                rule: "RTN105",
                message: "Return has no type annotation in signature.".to_string(),
                range: return_entry.entry_range.into(),
                fix: None,
                symbol: host.name.clone(),
            });
        }

        if matches!(config.type_annotation_style, Some(TypeAnnotationStyle::Docstring)) && signature_type.is_some() {
            diagnostics.push(Diagnostic {
                rule: "RTN106",
                message: "Return has a type annotation in signature; types belong in the docstring.".to_string(),
                range: return_entry.entry_range.into(),
                fix: None,
                symbol: host.name.clone(),
            });
        }
    }

    diagnostics
}

fn check_yield_rules(
    source: &Source,
    host: &DocstringHost,
    semantic: &docstring_cst::semantic::SemanticView,
    config: AnalysisConfig,
) -> Vec<Diagnostic> {
    if host.kind != HostKind::Function {
        return Vec::new();
    }

    let mut diagnostics = Vec::new();
    let yields_block = semantic
        .blocks()
        .iter()
        .find(|block| block.kind == docstring_cst::semantic::BlockKind::Yields);
    let yields = semantic.yields();

    if host.has_yield && yields_block.is_none() && yields.is_empty() {
        let insert_offset = semantic
            .close_quote()
            .map(|quote| quote.entry_range.start())
            .unwrap_or(host.docstring_range.end);
        diagnostics.push(Diagnostic {
            rule: "YLD001",
            message: "Missing Yields section in docstring.".to_string(),
            range: semantic
                .summary()
                .map(|summary| summary.entry_range.into())
                .unwrap_or(host.docstring_range),
            fix: Some(Fix {
                edits: vec![Edit::insert(
                    insert_offset,
                    yields_section_stub(source, host, semantic.style(), yield_type_annotation(source, host)),
                )],
                applicability: Applicability::Unsafe,
            }),
            symbol: host.name.clone(),
        });
    }

    if !host.has_yield
        && let Some(block) = yields_block
    {
        diagnostics.push(Diagnostic {
            rule: "YLD002",
            message: "Unnecessary Yields section in docstring.".to_string(),
            range: block.name_range.into(),
            fix: Some(Fix {
                edits: vec![Edit {
                    range: block.entry_range.into(),
                    replacement: String::new(),
                }],
                applicability: Applicability::Safe,
            }),
            symbol: host.name.clone(),
        });
    }

    for yield_entry in yields {
        let has_description = yield_entry
            .description_range
            .and_then(|range| source.slice(range))
            .is_some_and(|text| !text.trim().is_empty());
        if !has_description {
            diagnostics.push(Diagnostic {
                rule: "YLD003",
                message: "Yields section has no description.".to_string(),
                range: yield_entry
                    .type_range
                    .or(yield_entry.description_range)
                    .unwrap_or(yield_entry.entry_range)
                    .into(),
                fix: None,
                symbol: host.name.clone(),
            });
        }
    }

    let signature_type = yield_type_annotation(source, host);
    for yield_entry in yields {
        let doc_type = yield_entry
            .type_range
            .and_then(|range| source.slice(range))
            .map(str::trim)
            .filter(|text| !text.is_empty());
        match (doc_type, signature_type) {
            (Some(doc_type), Some(signature_type)) if !types_match(doc_type, signature_type) => {
                diagnostics.push(Diagnostic {
                    rule: "YLD101",
                    message: format!("Docstring yield type '{doc_type}' does not match type hint '{signature_type}'."),
                    range: yield_entry.type_range.unwrap().into(),
                    fix: Some(Fix {
                        edits: vec![Edit {
                            range: yield_entry.type_range.unwrap().into(),
                            replacement: signature_type.to_string(),
                        }],
                        applicability: Applicability::Unsafe,
                    }),
                    symbol: host.name.clone(),
                });
            }
            (None, None) if config.type_annotation_style.is_none() => {
                diagnostics.push(Diagnostic {
                    rule: "YLD102",
                    message: "Yield type not in docstring or signature.".to_string(),
                    range: yield_entry.entry_range.into(),
                    fix: None,
                    symbol: host.name.clone(),
                });
            }
            _ => {}
        }

        if matches!(
            config.type_annotation_style,
            Some(TypeAnnotationStyle::Docstring | TypeAnnotationStyle::Both)
        ) && doc_type.is_none()
        {
            diagnostics.push(Diagnostic {
                rule: "YLD103",
                message: "Yield has no type in docstring.".to_string(),
                range: yield_entry.entry_range.into(),
                fix: signature_type.map(|signature_type| Fix {
                    edits: vec![Edit::insert(
                        yield_entry.entry_range.start(),
                        return_type_insert_text(semantic.style(), signature_type),
                    )],
                    applicability: Applicability::Unsafe,
                }),
                symbol: host.name.clone(),
            });
        }

        if matches!(config.type_annotation_style, Some(TypeAnnotationStyle::Signature))
            && doc_type.is_some()
            && signature_type.is_some()
        {
            let type_range = yield_entry.type_range.unwrap();
            diagnostics.push(Diagnostic {
                rule: "YLD104",
                message: "Redundant yield type in docstring; type annotation exists in signature.".to_string(),
                range: type_range.into(),
                fix: Some(Fix {
                    edits: vec![Edit {
                        range: redundant_type_delete_range(source, semantic.style(), type_range),
                        replacement: String::new(),
                    }],
                    applicability: Applicability::Safe,
                }),
                symbol: host.name.clone(),
            });
        }

        if matches!(
            config.type_annotation_style,
            Some(TypeAnnotationStyle::Signature | TypeAnnotationStyle::Both)
        ) && signature_type.is_none()
        {
            diagnostics.push(Diagnostic {
                rule: "YLD105",
                message: "Yield has no type annotation in signature.".to_string(),
                range: yield_entry.entry_range.into(),
                fix: None,
                symbol: host.name.clone(),
            });
        }

        if matches!(config.type_annotation_style, Some(TypeAnnotationStyle::Docstring)) && signature_type.is_some() {
            diagnostics.push(Diagnostic {
                rule: "YLD106",
                message: "Yield has a type annotation in signature; types belong in the docstring.".to_string(),
                range: yield_entry.entry_range.into(),
                fix: None,
                symbol: host.name.clone(),
            });
        }
    }

    diagnostics
}

fn return_annotation<'a>(source: &'a Source, host: &DocstringHost) -> Option<&'a str> {
    let range = host.return_annotation_range?;
    let text = source.slice(range.into_text_range())?.trim();
    let annotation = text.strip_prefix("->")?.trim();
    (!annotation.is_empty()).then_some(annotation)
}

fn meaningful_return_annotation<'a>(source: &'a Source, host: &DocstringHost) -> Option<&'a str> {
    let annotation = return_annotation(source, host)?;
    (!annotation.is_empty() && annotation != "None").then_some(annotation)
}

fn types_match(docstring_type: &str, signature_type: &str) -> bool {
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

fn return_type_insert_text(style: DocstringStyle, signature_type: &str) -> String {
    match style {
        DocstringStyle::Numpy => format!("{signature_type}\n"),
        _ => format!("{signature_type}: "),
    }
}

fn redundant_type_delete_range(source: &Source, style: DocstringStyle, type_range: TextRange) -> Range {
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

fn returns_section_stub(
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

fn yields_section_stub(
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

fn yield_type_annotation<'a>(source: &'a Source, host: &DocstringHost) -> Option<&'a str> {
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

fn line_indent_before(source: &str, offset: usize) -> &str {
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    &source[line_start..offset]
}

fn check_summary_rules(
    source: &Source,
    host: &DocstringHost,
    semantic: &docstring_cst::semantic::SemanticView,
) -> Vec<Diagnostic> {
    let Some(summary) = semantic.summary() else {
        return vec![Diagnostic {
            rule: "SUM001",
            message: "Docstring has no summary line.".to_string(),
            range: host.docstring_range,
            fix: None,
            symbol: host.name.clone(),
        }];
    };

    let summary_range = summary.entry_range;
    let summary_text = source.slice(summary_range).unwrap_or("").trim();
    if summary_text.is_empty() {
        return vec![Diagnostic {
            rule: "SUM001",
            message: "Docstring has no summary line.".to_string(),
            range: host.docstring_range,
            fix: None,
            symbol: host.name.clone(),
        }];
    }

    if matches!(summary_text.chars().next_back(), Some('.' | '!' | '?')) {
        return Vec::new();
    }

    vec![Diagnostic {
        rule: "SUM002",
        message: "Summary should end with a period.".to_string(),
        range: summary_range.into(),
        fix: Some(Fix {
            edits: vec![Edit::insert(summary_insert_offset(source, summary_range), ".")],
            applicability: Applicability::Safe,
        }),
        symbol: host.name.clone(),
    }]
}

fn summary_insert_offset(source: &Source, range: TextRange) -> usize {
    let Some(text) = source.slice(range) else {
        return range.end();
    };
    let trailing_whitespace_len = text.len() - text.trim_end().len();
    range.end() - trailing_whitespace_len
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
    fn emits_prm201_for_default_parameter_missing_optional() {
        let source = r#"def value(x: int = 0) -> None:
    """Do something.

    Args:
        x (int): The value.
    """
    pass
"#;

        let report = analyze_source(source);
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
