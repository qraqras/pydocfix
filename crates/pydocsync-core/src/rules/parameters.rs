use docstring_cst::semantic::{BlockKind, SemanticView};
use docstring_cst::{DocstringStyle, Source, TextRange};

use crate::{
    AnalysisConfig, Applicability, Diagnostic, DocstringHost, Edit, Fix, HostKind, Range, SignatureParameter,
    line_indent_before,
};

use super::RuleContext;

pub(crate) fn check_parameter_rules(
    source: &Source,
    host: &DocstringHost,
    semantic: &SemanticView,
    _config: AnalysisConfig,
) -> Vec<Diagnostic> {
    if host.kind != HostKind::Function {
        return Vec::new();
    }

    let ctx = RuleContext { source, host, semantic };

    let mut diagnostics = Vec::new();
    prm001(&ctx, &mut diagnostics);
    prm002(&ctx, &mut diagnostics);
    prm003(&ctx, &mut diagnostics);
    prm004(&ctx, &mut diagnostics);
    prm005(&ctx, &mut diagnostics);
    prm006(&ctx, &mut diagnostics);
    prm007(&ctx, &mut diagnostics);
    prm009(&ctx, &mut diagnostics);

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
            rule: "arg-section-missing",
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
        rule: "arg-section-extra",
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
                rule: "arg-receiver",
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
                rule: "arg-missing",
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
                rule: "arg-extra",
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
            rule: "arg-order",
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
                rule: "arg-duplicate",
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
            rule: "arg-vararg-marker",
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
            })
        })
        .collect()
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
