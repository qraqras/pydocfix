use docstring_cst::Source;
use docstring_cst::semantic::{Return, SemanticBlock, SemanticView};

use crate::{
    AnalysisConfig, Applicability, Diagnostic, DocstringHost, Edit, Fix, HostKind, TypeAnnotationStyle,
    meaningful_return_annotation, redundant_type_delete_range, return_annotation, return_type_insert_text,
    returns_section_stub, types_match,
};

use super::RuleContext;

pub(crate) fn check_return_rules(
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
    rtn001(&ctx, &mut diagnostics);
    rtn002(&ctx, &mut diagnostics);
    rtn003(&ctx, &mut diagnostics);
    rtn101(&ctx, &mut diagnostics);
    rtn102(&ctx, &mut diagnostics);
    rtn103(&ctx, &mut diagnostics);
    rtn104(&ctx, &mut diagnostics);
    rtn105(&ctx, &mut diagnostics);
    rtn106(&ctx, &mut diagnostics);

    diagnostics
}

fn rtn001(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    if ctx.host.has_yield || returns_block(ctx.semantic).is_some() || !ctx.semantic.returns().is_empty() {
        return;
    }
    let Some(return_annotation) = meaningful_return_annotation(ctx.source, ctx.host) else {
        return;
    };
    let insert_offset = ctx
        .semantic
        .close_quote()
        .map(|quote| quote.entry_range.start())
        .unwrap_or(ctx.host.docstring_range.end);
    diagnostics.push(Diagnostic {
        rule: "RTN001",
        message: "Missing Returns section in docstring.".to_string(),
        range: ctx
            .semantic
            .summary()
            .map(|summary| summary.entry_range.into())
            .unwrap_or(ctx.host.docstring_range),
        fix: Some(Fix {
            edits: vec![Edit::insert(
                insert_offset,
                returns_section_stub(ctx.source, ctx.host, ctx.semantic.style(), return_annotation),
            )],
            applicability: Applicability::Unsafe,
        }),
        symbol: ctx.host.name.clone(),
    });
}

fn rtn002(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    if ctx.host.has_return_value {
        return;
    }
    let Some(block) = returns_block(ctx.semantic) else {
        return;
    };
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
        symbol: ctx.host.name.clone(),
    });
}

fn rtn003(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    for return_entry in ctx.semantic.returns() {
        rtn003_entry(ctx, diagnostics, return_entry);
    }
}

fn rtn003_entry(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>, return_entry: &Return) {
    let has_description = return_entry
        .description_range
        .and_then(|range| ctx.source.slice(range))
        .is_some_and(|text| !text.trim().is_empty());
    if has_description {
        return;
    }
    diagnostics.push(Diagnostic {
        rule: "RTN003",
        message: "Returns section has no description.".to_string(),
        range: return_entry
            .type_range
            .or(return_entry.description_range)
            .unwrap_or(return_entry.entry_range)
            .into(),
        fix: None,
        symbol: ctx.host.name.clone(),
    });
}

fn rtn101(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let signature_type = return_annotation(ctx.source, ctx.host);
    for return_entry in ctx.semantic.returns() {
        rtn101_entry(ctx, diagnostics, return_entry, signature_type);
    }
}

fn rtn101_entry(
    ctx: &RuleContext<'_>,
    diagnostics: &mut Vec<Diagnostic>,
    return_entry: &Return,
    signature_type: Option<&str>,
) {
    let Some(doc_type) = docstring_return_type(ctx.source, return_entry) else {
        return;
    };
    let Some(signature_type) = signature_type else {
        return;
    };
    if types_match(doc_type, signature_type) {
        return;
    }
    let Some(type_range) = return_entry.type_range else {
        return;
    };
    diagnostics.push(Diagnostic {
        rule: "RTN101",
        message: format!("Docstring return type '{doc_type}' does not match type hint '{signature_type}'."),
        range: type_range.into(),
        fix: Some(Fix {
            edits: vec![Edit {
                range: type_range.into(),
                replacement: signature_type.to_string(),
            }],
            applicability: Applicability::Unsafe,
        }),
        symbol: ctx.host.name.clone(),
    });
}

fn rtn102(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let signature_type = return_annotation(ctx.source, ctx.host);
    for return_entry in ctx.semantic.returns() {
        if signature_type.is_some() || return_entry.type_range.is_some() || ctx.config.type_annotation_style.is_some() {
            continue;
        }
        diagnostics.push(Diagnostic {
            rule: "RTN102",
            message: "Return type not in docstring or signature.".to_string(),
            range: return_entry.entry_range.into(),
            fix: None,
            symbol: ctx.host.name.clone(),
        });
    }
}

fn rtn103(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let signature_type = return_annotation(ctx.source, ctx.host);
    for return_entry in ctx.semantic.returns() {
        if !matches!(
            ctx.config.type_annotation_style,
            Some(TypeAnnotationStyle::Docstring | TypeAnnotationStyle::Both)
        ) || return_entry.type_range.is_some()
        {
            continue;
        }
        diagnostics.push(Diagnostic {
            rule: "RTN103",
            message: "Return has no type in docstring.".to_string(),
            range: return_entry.entry_range.into(),
            fix: signature_type.map(|signature_type| Fix {
                edits: vec![Edit::insert(
                    return_entry.entry_range.start(),
                    return_type_insert_text(ctx.semantic.style(), signature_type),
                )],
                applicability: Applicability::Unsafe,
            }),
            symbol: ctx.host.name.clone(),
        });
    }
}

fn rtn104(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let signature_type = return_annotation(ctx.source, ctx.host);
    for return_entry in ctx.semantic.returns() {
        if !matches!(ctx.config.type_annotation_style, Some(TypeAnnotationStyle::Signature)) || signature_type.is_none()
        {
            continue;
        }
        let Some(type_range) = return_entry.type_range else {
            continue;
        };
        diagnostics.push(Diagnostic {
            rule: "RTN104",
            message: "Redundant return type in docstring; type annotation exists in signature.".to_string(),
            range: type_range.into(),
            fix: Some(Fix {
                edits: vec![Edit {
                    range: redundant_type_delete_range(ctx.source, ctx.semantic.style(), type_range),
                    replacement: String::new(),
                }],
                applicability: Applicability::Safe,
            }),
            symbol: ctx.host.name.clone(),
        });
    }
}

fn rtn105(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let signature_type = return_annotation(ctx.source, ctx.host);
    for return_entry in ctx.semantic.returns() {
        if !matches!(
            ctx.config.type_annotation_style,
            Some(TypeAnnotationStyle::Signature | TypeAnnotationStyle::Both)
        ) || signature_type.is_some()
        {
            continue;
        }
        diagnostics.push(Diagnostic {
            rule: "RTN105",
            message: "Return has no type annotation in signature.".to_string(),
            range: return_entry.entry_range.into(),
            fix: None,
            symbol: ctx.host.name.clone(),
        });
    }
}

fn rtn106(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let signature_type = return_annotation(ctx.source, ctx.host);
    for return_entry in ctx.semantic.returns() {
        if !matches!(ctx.config.type_annotation_style, Some(TypeAnnotationStyle::Docstring)) || signature_type.is_none()
        {
            continue;
        }
        diagnostics.push(Diagnostic {
            rule: "RTN106",
            message: "Return has a type annotation in signature; types belong in the docstring.".to_string(),
            range: return_entry.entry_range.into(),
            fix: None,
            symbol: ctx.host.name.clone(),
        });
    }
}

fn docstring_return_type<'a>(source: &'a Source, return_entry: &Return) -> Option<&'a str> {
    return_entry
        .type_range
        .and_then(|range| source.slice(range))
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn returns_block(semantic: &SemanticView) -> Option<&SemanticBlock> {
    semantic
        .blocks()
        .iter()
        .find(|block| block.kind == docstring_cst::semantic::BlockKind::Returns)
}
