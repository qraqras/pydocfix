use docstring_cst::semantic::{BlockKind, SemanticView};
use docstring_cst::{DocstringStyle, Source, TextRange};

use crate::{
    AnalysisConfig, Applicability, Diagnostic, DocstringHost, Edit, Fix, SignatureParameter, line_indent_before,
};

use super::RuleContext;

pub(crate) fn check_parameter_rules(
    source: &Source,
    host: &DocstringHost,
    semantic: &SemanticView,
    _config: AnalysisConfig,
) -> Vec<Diagnostic> {
    let ctx = RuleContext { source, host, semantic };

    let mut diagnostics = Vec::new();
    prm003(&ctx, &mut diagnostics);
    prm004(&ctx, &mut diagnostics);
    prm005(&ctx, &mut diagnostics);
    prm007(&ctx, &mut diagnostics);

    diagnostics
}

fn prm003(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.extend(
        documented_parameters(ctx.source, ctx.semantic)
            .into_iter()
            .filter(|documented_param| matches!(documented_param.name.as_str(), "self" | "cls"))
            .map(|documented_param| Diagnostic {
                rule: "args-receiver-documented",
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
        if documented_contains_unknown_names(ctx.host, &documented) {
            return;
        }
        for signature_param in documentable_signature_parameters(ctx.host) {
            if !is_required_named_parameter(signature_param) {
                continue;
            }
            if documented
                .iter()
                .any(|documented_param| bare_parameter_name(&documented_param.name) == signature_param.bare_name)
            {
                continue;
            }
            diagnostics.push(Diagnostic {
                rule: "args-param-missing",
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
    if has_signature_variadics(ctx.host) {
        return;
    }

    let signature_names = all_signature_bare_names(ctx.host);
    if signature_names.is_empty() {
        return;
    }
    diagnostics.extend(documented_parameters(ctx.source, ctx.semantic).into_iter().filter_map(
        move |documented_param| {
            let bare_name = bare_parameter_name(&documented_param.name);
            if !is_plausible_parameter_name(bare_name) {
                return None;
            }
            (!signature_names.iter().any(|name| *name == bare_name)).then(|| Diagnostic {
                rule: "args-param-extra",
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

fn prm007(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let documented = documented_parameters(ctx.source, ctx.semantic);
    let mut seen = Vec::new();
    for documented_param in &documented {
        if seen.contains(&documented_param.name.as_str()) {
            diagnostics.push(Diagnostic {
                rule: "args-param-duplicate",
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
        .flat_map(|param| {
            param.name_ranges.iter().filter_map(|&name_range| {
                let name = source.slice(name_range)?.trim().to_string();
                Some(DocumentedParameter {
                    name,
                    entry_range: param.entry_range,
                    name_range,
                })
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

fn is_required_named_parameter(param: &SignatureParameter) -> bool {
    !param.is_vararg && !param.is_kwarg && param.default_value.is_none()
}

fn documented_contains_unknown_names(host: &DocstringHost, documented: &[DocumentedParameter]) -> bool {
    let signature_names = host
        .signature_parameters
        .iter()
        .map(|param| param.bare_name.as_str())
        .collect::<Vec<_>>();
    documented
        .iter()
        .map(|param| bare_parameter_name(&param.name))
        .any(|name| !signature_names.contains(&name))
}

fn has_signature_variadics(host: &DocstringHost) -> bool {
    host.signature_parameters
        .iter()
        .any(|param| param.is_vararg || param.is_kwarg)
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

fn is_plausible_parameter_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == b'_') && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
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
