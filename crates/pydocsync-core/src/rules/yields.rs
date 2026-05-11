use docstring_cst::Source;
use docstring_cst::semantic::{SemanticBlock, SemanticView};

use crate::{
    AnalysisConfig, Applicability, Diagnostic, DocstringHost, Edit, Fix, HostKind, yield_type_annotation,
    yields_section_stub,
};

use super::RuleContext;

pub(crate) fn check_yield_rules(
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
    yld001(&ctx, &mut diagnostics);
    yld002(&ctx, &mut diagnostics);

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
        rule: "yield-missing",
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
        rule: "yield-extra",
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

fn yields_block(semantic: &SemanticView) -> Option<&SemanticBlock> {
    semantic
        .blocks()
        .iter()
        .find(|block| block.kind == docstring_cst::semantic::BlockKind::Yields)
}
