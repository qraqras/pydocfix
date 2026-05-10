use docstring_cst::Source;
use docstring_cst::semantic::{SemanticBlock, SemanticView, Yield};

use crate::{
    AnalysisConfig, Applicability, Diagnostic, DocstringHost, Edit, Fix, HostKind, TypeAnnotationStyle,
    redundant_type_delete_range, return_type_insert_text, types_match, yield_type_annotation, yields_section_stub,
};

use super::RuleContext;

pub(crate) fn check_yield_rules(
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
    yld001(&ctx, &mut diagnostics);
    yld002(&ctx, &mut diagnostics);
    yld003(&ctx, &mut diagnostics);
    yld101(&ctx, &mut diagnostics);
    yld102(&ctx, &mut diagnostics);
    yld103(&ctx, &mut diagnostics);
    yld104(&ctx, &mut diagnostics);
    yld105(&ctx, &mut diagnostics);
    yld106(&ctx, &mut diagnostics);

    diagnostics
}

fn yld001(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    if !ctx.host.has_yield || yields_block(ctx.semantic).is_some() || !ctx.semantic.yields().is_empty() {
        return;
    }
    let insert_offset = ctx
        .semantic
        .close_quote()
        .map(|quote| quote.entry_range.start())
        .unwrap_or(ctx.host.docstring_range.end);
    diagnostics.push(Diagnostic {
        rule: "YLD001",
        message: "Missing Yields section in docstring.".to_string(),
        range: ctx
            .semantic
            .summary()
            .map(|summary| summary.entry_range.into())
            .unwrap_or(ctx.host.docstring_range),
        fix: Some(Fix {
            edits: vec![Edit::insert(
                insert_offset,
                yields_section_stub(
                    ctx.source,
                    ctx.host,
                    ctx.semantic.style(),
                    yield_type_annotation(ctx.source, ctx.host),
                ),
            )],
            applicability: Applicability::Unsafe,
        }),
        symbol: ctx.host.name.clone(),
    });
}

fn yld002(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    if ctx.host.has_yield {
        return;
    }
    let Some(block) = yields_block(ctx.semantic) else {
        return;
    };
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
        symbol: ctx.host.name.clone(),
    });
}

fn yld003(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    for yield_entry in ctx.semantic.yields() {
        yld003_entry(ctx, diagnostics, yield_entry);
    }
}

fn yld003_entry(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>, yield_entry: &Yield) {
    let has_description = yield_entry
        .description_range
        .and_then(|range| ctx.source.slice(range))
        .is_some_and(|text| !text.trim().is_empty());
    if has_description {
        return;
    }
    diagnostics.push(Diagnostic {
        rule: "YLD003",
        message: "Yields section has no description.".to_string(),
        range: yield_entry
            .type_range
            .or(yield_entry.description_range)
            .unwrap_or(yield_entry.entry_range)
            .into(),
        fix: None,
        symbol: ctx.host.name.clone(),
    });
}

fn yld101(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let signature_type = yield_type_annotation(ctx.source, ctx.host);
    for yield_entry in ctx.semantic.yields() {
        yld101_entry(ctx, diagnostics, yield_entry, signature_type);
    }
}

fn yld101_entry(
    ctx: &RuleContext<'_>,
    diagnostics: &mut Vec<Diagnostic>,
    yield_entry: &Yield,
    signature_type: Option<&str>,
) {
    let Some(doc_type) = docstring_yield_type(ctx.source, yield_entry) else {
        return;
    };
    let Some(signature_type) = signature_type else {
        return;
    };
    if types_match(doc_type, signature_type) {
        return;
    }
    let Some(type_range) = yield_entry.type_range else {
        return;
    };
    diagnostics.push(Diagnostic {
        rule: "YLD101",
        message: format!("Docstring yield type '{doc_type}' does not match type hint '{signature_type}'."),
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

fn yld102(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let signature_type = yield_type_annotation(ctx.source, ctx.host);
    for yield_entry in ctx.semantic.yields() {
        if signature_type.is_some() || yield_entry.type_range.is_some() || ctx.config.type_annotation_style.is_some() {
            continue;
        }
        diagnostics.push(Diagnostic {
            rule: "YLD102",
            message: "Yield type not in docstring or signature.".to_string(),
            range: yield_entry.entry_range.into(),
            fix: None,
            symbol: ctx.host.name.clone(),
        });
    }
}

fn yld103(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let signature_type = yield_type_annotation(ctx.source, ctx.host);
    for yield_entry in ctx.semantic.yields() {
        if !matches!(
            ctx.config.type_annotation_style,
            Some(TypeAnnotationStyle::Docstring | TypeAnnotationStyle::Both)
        ) || yield_entry.type_range.is_some()
        {
            continue;
        }
        diagnostics.push(Diagnostic {
            rule: "YLD103",
            message: "Yield has no type in docstring.".to_string(),
            range: yield_entry.entry_range.into(),
            fix: signature_type.map(|signature_type| Fix {
                edits: vec![Edit::insert(
                    yield_entry.entry_range.start(),
                    return_type_insert_text(ctx.semantic.style(), signature_type),
                )],
                applicability: Applicability::Unsafe,
            }),
            symbol: ctx.host.name.clone(),
        });
    }
}

fn yld104(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let signature_type = yield_type_annotation(ctx.source, ctx.host);
    for yield_entry in ctx.semantic.yields() {
        if !matches!(ctx.config.type_annotation_style, Some(TypeAnnotationStyle::Signature)) || signature_type.is_none()
        {
            continue;
        }
        let Some(type_range) = yield_entry.type_range else {
            continue;
        };
        diagnostics.push(Diagnostic {
            rule: "YLD104",
            message: "Redundant yield type in docstring; type annotation exists in signature.".to_string(),
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

fn yld105(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let signature_type = yield_type_annotation(ctx.source, ctx.host);
    for yield_entry in ctx.semantic.yields() {
        if !matches!(
            ctx.config.type_annotation_style,
            Some(TypeAnnotationStyle::Signature | TypeAnnotationStyle::Both)
        ) || signature_type.is_some()
        {
            continue;
        }
        diagnostics.push(Diagnostic {
            rule: "YLD105",
            message: "Yield has no type annotation in signature.".to_string(),
            range: yield_entry.entry_range.into(),
            fix: None,
            symbol: ctx.host.name.clone(),
        });
    }
}

fn yld106(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let signature_type = yield_type_annotation(ctx.source, ctx.host);
    for yield_entry in ctx.semantic.yields() {
        if !matches!(ctx.config.type_annotation_style, Some(TypeAnnotationStyle::Docstring)) || signature_type.is_none()
        {
            continue;
        }
        diagnostics.push(Diagnostic {
            rule: "YLD106",
            message: "Yield has a type annotation in signature; types belong in the docstring.".to_string(),
            range: yield_entry.entry_range.into(),
            fix: None,
            symbol: ctx.host.name.clone(),
        });
    }
}

fn docstring_yield_type<'a>(source: &'a Source, yield_entry: &Yield) -> Option<&'a str> {
    yield_entry
        .type_range
        .and_then(|range| source.slice(range))
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn yields_block(semantic: &SemanticView) -> Option<&SemanticBlock> {
    semantic
        .blocks()
        .iter()
        .find(|block| block.kind == docstring_cst::semantic::BlockKind::Yields)
}
