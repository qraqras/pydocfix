use docstring_cst::semantic::{BlockKind, SemanticView};
use docstring_cst::{DocstringStyle, Source, TextRange};

use crate::{
    AnalysisConfig, Applicability, Diagnostic, DocstringHost, Edit, Fix, HostKind, Range, SignatureParameter,
    TypeAnnotationStyle, line_indent_before, types_match,
};

use super::RuleContext;

pub(crate) fn check_parameter_rules(
    source: &Source,
    host: &DocstringHost,
    semantic: &SemanticView,
    config: AnalysisConfig,
) -> Vec<Diagnostic> {
    if host.kind != HostKind::Function {
        return Vec::new();
    }

    let ctx = RuleContext {
        source,
        host,
        semantic,
        config,
    };

    let mut diagnostics = Vec::new();
    prm001(&ctx, &mut diagnostics);
    prm002(&ctx, &mut diagnostics);
    prm003(&ctx, &mut diagnostics);
    prm004(&ctx, &mut diagnostics);
    prm005(&ctx, &mut diagnostics);
    prm006(&ctx, &mut diagnostics);
    prm007(&ctx, &mut diagnostics);
    prm008(&ctx, &mut diagnostics);
    prm009(&ctx, &mut diagnostics);
    prm101(&ctx, &mut diagnostics);
    prm102(&ctx, &mut diagnostics);
    prm103(&ctx, &mut diagnostics);
    prm104(&ctx, &mut diagnostics);
    prm105(&ctx, &mut diagnostics);
    prm106(&ctx, &mut diagnostics);
    prm201(&ctx, &mut diagnostics);
    prm202(&ctx, &mut diagnostics);

    diagnostics
}

fn prm001(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let signature_params = documentable_signature_parameters(ctx.host);
    if ctx.host.name.as_deref() != Some("__init__")
        && !signature_params.is_empty()
        && parameter_block(ctx.semantic).is_none()
        && ctx.semantic.parameters().is_empty()
    {
        let insert_offset = ctx
            .semantic
            .close_quote()
            .map(|quote| quote.entry_range.start())
            .unwrap_or(ctx.host.docstring_range.end);
        diagnostics.push(Diagnostic {
            rule: "PRM001",
            message: "Missing Args/Parameters section in docstring.".to_string(),
            range: ctx
                .semantic
                .summary()
                .map(|summary| summary.entry_range.into())
                .unwrap_or(ctx.host.docstring_range),
            fix: Some(Fix {
                edits: vec![Edit::insert(
                    insert_offset,
                    args_section_stub(ctx.source, ctx.host, ctx.semantic.style()),
                )],
                applicability: Applicability::Unsafe,
            }),
            symbol: ctx.host.name.clone(),
        });
    }
}

fn prm002(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    if !documentable_signature_parameters(ctx.host).is_empty() {
        return;
    }
    let Some(block) = parameter_block(ctx.semantic) else {
        return;
    };
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
        symbol: ctx.host.name.clone(),
    });
}

fn prm003(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.extend(
        documented_parameters(ctx.source, ctx.semantic)
            .into_iter()
            .filter(|documented_param| matches!(documented_param.name.as_str(), "self" | "cls"))
            .map(|documented_param| Diagnostic {
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
                symbol: ctx.host.name.clone(),
            }),
    );
}

fn prm004(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let documented = documented_parameters(ctx.source, ctx.semantic);
    if let Some(block) = parameter_block(ctx.semantic)
        && !documented.is_empty()
    {
        for signature_param in documentable_signature_parameters(ctx.host) {
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
                        parameter_entry_append_text(
                            ctx.source,
                            block.name_range,
                            ctx.semantic.style(),
                            signature_param,
                        ),
                    )],
                    applicability: Applicability::Unsafe,
                }),
                symbol: ctx.host.name.clone(),
            });
        }
    }
}

fn prm005(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let signature_names = all_signature_bare_names(ctx.host);
    diagnostics.extend(documented_parameters(ctx.source, ctx.semantic).into_iter().filter_map(
        move |documented_param| {
            let bare_name = bare_parameter_name(&documented_param.name);
            (!signature_names.iter().any(|name| *name == bare_name)).then(|| Diagnostic {
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
                symbol: ctx.host.name.clone(),
            })
        },
    ));
}

fn prm006(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let documented = documented_parameters(ctx.source, ctx.semantic);
    let signature_order: Vec<&str> = documentable_signature_parameters(ctx.host)
        .into_iter()
        .map(|param| param.bare_name.as_str())
        .collect();
    if signature_order.is_empty() || documented.len() < 2 {
        return;
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
        return;
    }

    let fix = reorder_parameters_fix(ctx.source, &documented, &signature_order);
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
            symbol: ctx.host.name.clone(),
        });
    }
}

fn prm007(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let documented = documented_parameters(ctx.source, ctx.semantic);
    let mut seen = Vec::new();
    for documented_param in &documented {
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
                symbol: ctx.host.name.clone(),
            });
        } else {
            seen.push(documented_param.name.as_str());
        }
    }
}

fn prm008(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.extend(
        documented_parameters(ctx.source, ctx.semantic)
            .into_iter()
            .filter(|param| {
                param
                    .description_range
                    .and_then(|range| ctx.source.slice(range))
                    .is_none_or(|description| description.trim().is_empty())
            })
            .map(|param| Diagnostic {
                rule: "PRM008",
                message: format!("Parameter '{}' has no description.", param.name),
                range: param.name_range.into(),
                fix: None,
                symbol: ctx.host.name.clone(),
            }),
    );
}

fn prm009(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let documented = documented_parameters(ctx.source, ctx.semantic);
    for documented_param in &documented {
        if documented_param.name.starts_with('*') {
            continue;
        }
        let Some(signature_param) = ctx
            .host
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
            symbol: ctx.host.name.clone(),
        });
    }
}

fn prm101(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.extend(documented_parameters(ctx.source, ctx.semantic).into_iter().filter_map(
        move |documented_param| {
            let (signature_param, doc_type, signature_type) = parameter_types(ctx.source, ctx.host, &documented_param)?;
            if types_match(doc_type, signature_type) {
                return None;
            }
            let type_range = documented_param.type_range?;
            Some(Diagnostic {
                rule: "PRM101",
                message: format!(
                    "Docstring type '{doc_type}' does not match type hint '{signature_type}' for parameter '{}'.",
                    documented_param.name
                ),
                range: type_range.into(),
                fix: Some(Fix {
                    edits: vec![Edit {
                        range: type_range.into(),
                        replacement: signature_param.annotation.clone().unwrap_or_default(),
                    }],
                    applicability: Applicability::Unsafe,
                }),
                symbol: ctx.host.name.clone(),
            })
        },
    ));
}

fn prm102(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.extend(documented_parameters(ctx.source, ctx.semantic).into_iter().filter_map(
        move |documented_param| {
            if ctx.config.type_annotation_style.is_some() {
                return None;
            }
            let signature_param = signature_parameter(ctx.host, &documented_param)?;
            let doc_type = docstring_parameter_type(ctx.source, &documented_param);
            (doc_type.is_none() && signature_param.annotation.is_none()).then(|| Diagnostic {
                rule: "PRM102",
                message: format!(
                    "Parameter '{}' has no type in docstring or signature.",
                    documented_param.name
                ),
                range: documented_param.name_range.into(),
                fix: None,
                symbol: ctx.host.name.clone(),
            })
        },
    ));
}

fn prm103(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.extend(documented_parameters(ctx.source, ctx.semantic).into_iter().filter_map(
        move |documented_param| {
            if !matches!(
                ctx.config.type_annotation_style,
                Some(TypeAnnotationStyle::Docstring | TypeAnnotationStyle::Both)
            ) || docstring_parameter_type(ctx.source, &documented_param).is_some()
            {
                return None;
            }
            let signature_param = signature_parameter(ctx.host, &documented_param)?;
            Some(Diagnostic {
                rule: "PRM103",
                message: format!("Parameter '{}' has no type in docstring.", documented_param.name),
                range: documented_param.name_range.into(),
                fix: signature_param.annotation.as_deref().map(|signature_type| Fix {
                    edits: vec![Edit::insert(
                        documented_param.name_range.end(),
                        parameter_type_insert_text(ctx.semantic.style(), signature_type),
                    )],
                    applicability: Applicability::Unsafe,
                }),
                symbol: ctx.host.name.clone(),
            })
        },
    ));
}

fn prm104(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.extend(documented_parameters(ctx.source, ctx.semantic).into_iter().filter_map(
        move |documented_param| {
            if !matches!(ctx.config.type_annotation_style, Some(TypeAnnotationStyle::Signature)) {
                return None;
            }
            let signature_param = signature_parameter(ctx.host, &documented_param)?;
            let type_range = documented_param.type_range?;
            (docstring_parameter_type(ctx.source, &documented_param).is_some() && signature_param.annotation.is_some())
                .then(|| Diagnostic {
                    rule: "PRM104",
                    message: format!("Parameter '{}' has redundant type in docstring.", documented_param.name),
                    range: type_range.into(),
                    fix: Some(Fix {
                        edits: vec![Edit {
                            range: parameter_type_delete_range(
                                ctx.source,
                                ctx.semantic.style(),
                                &documented_param,
                                type_range,
                            ),
                            replacement: String::new(),
                        }],
                        applicability: Applicability::Safe,
                    }),
                    symbol: ctx.host.name.clone(),
                })
        },
    ));
}

fn prm105(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.extend(documented_parameters(ctx.source, ctx.semantic).into_iter().filter_map(
        move |documented_param| {
            if !matches!(
                ctx.config.type_annotation_style,
                Some(TypeAnnotationStyle::Signature | TypeAnnotationStyle::Both)
            ) {
                return None;
            }
            let signature_param = signature_parameter(ctx.host, &documented_param)?;
            signature_param.annotation.is_none().then(|| Diagnostic {
                rule: "PRM105",
                message: format!(
                    "Parameter '{}' has no type annotation in signature.",
                    documented_param.name
                ),
                range: documented_param.name_range.into(),
                fix: None,
                symbol: ctx.host.name.clone(),
            })
        },
    ));
}

fn prm106(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.extend(documented_parameters(ctx.source, ctx.semantic).into_iter().filter_map(
        move |documented_param| {
            if !matches!(ctx.config.type_annotation_style, Some(TypeAnnotationStyle::Docstring)) {
                return None;
            }
            let signature_param = signature_parameter(ctx.host, &documented_param)?;
            signature_param.annotation.is_some().then(|| Diagnostic {
                rule: "PRM106",
                message: format!(
                    "Parameter '{}' has a type annotation in signature; types belong in the docstring.",
                    documented_param.name
                ),
                range: documented_param.name_range.into(),
                fix: None,
                symbol: ctx.host.name.clone(),
            })
        },
    ));
}

fn prm201(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.extend(documented_parameters(ctx.source, ctx.semantic).into_iter().filter_map(
        move |documented_param| {
            if !ctx.config.enable_prm201 {
                return None;
            }
            let signature_param = signature_parameter(ctx.host, &documented_param)?;
            signature_param.default_value.as_ref()?;
            (documented_param.type_range.is_some() && documented_param.optional_range.is_none()).then(|| Diagnostic {
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
                symbol: ctx.host.name.clone(),
            })
        },
    ));
}

fn prm202(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.extend(documented_parameters(ctx.source, ctx.semantic).into_iter().filter_map(
        move |documented_param| {
            if !ctx.config.enable_prm202 || documented_parameter_mentions_default(ctx.source, &documented_param) {
                return None;
            }
            let signature_param = signature_parameter(ctx.host, &documented_param)?;
            let default_value = signature_param.default_value.as_deref()?;
            Some(Diagnostic {
                rule: "PRM202",
                message: format!(
                    "Parameter '{}' has default value but docstring does not mention 'default'.",
                    documented_param.name
                ),
                range: documented_param.name_range.into(),
                fix: documented_param.description_range.and_then(|description_range| {
                    let description = ctx.source.slice(description_range)?.trim_end();
                    let suffix = if description.ends_with('.') { "" } else { "." };
                    Some(Fix {
                        edits: vec![Edit::insert(
                            description_range.end(),
                            format!("{suffix} Defaults to {default_value}."),
                        )],
                        applicability: Applicability::Unsafe,
                    })
                }),
                symbol: ctx.host.name.clone(),
            })
        },
    ));
}

fn signature_parameter<'a>(
    host: &'a DocstringHost,
    documented_param: &DocumentedParameter,
) -> Option<&'a SignatureParameter> {
    let bare_name = bare_parameter_name(&documented_param.name);
    host.signature_parameters
        .iter()
        .find(|param| param.bare_name == bare_name && !param.is_implicit_receiver)
}

fn docstring_parameter_type<'a>(source: &'a Source, documented_param: &DocumentedParameter) -> Option<&'a str> {
    documented_param
        .type_range
        .and_then(|range| source.slice(range))
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn parameter_types<'a>(
    source: &'a Source,
    host: &'a DocstringHost,
    documented_param: &DocumentedParameter,
) -> Option<(&'a SignatureParameter, &'a str, &'a str)> {
    let signature_param = signature_parameter(host, documented_param)?;
    let doc_type = docstring_parameter_type(source, documented_param)?;
    let signature_type = signature_param.annotation.as_deref()?;
    Some((signature_param, doc_type, signature_type))
}

fn parameter_block(semantic: &SemanticView) -> Option<&docstring_cst::semantic::SemanticBlock> {
    semantic
        .blocks()
        .iter()
        .find(|block| block.kind == BlockKind::Parameters)
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

fn documented_parameters(source: &Source, semantic: &SemanticView) -> Vec<DocumentedParameter> {
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

pub(crate) fn documentable_signature_parameters(host: &DocstringHost) -> Vec<&SignatureParameter> {
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

pub(crate) fn args_section_stub(source: &Source, host: &DocstringHost, style: DocstringStyle) -> String {
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
