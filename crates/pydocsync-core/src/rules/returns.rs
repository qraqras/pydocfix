use docstring_cst::Source;
use docstring_cst::semantic::{BlockKind, SemanticBlock, SemanticView};

use crate::{
    AnalysisConfig, Applicability, Diagnostic, DocstringHost, Edit, Fix, HostKind, meaningful_return_annotation,
    returns_section_stub,
};

use super::{RuleContext, has_other_section};

pub(crate) fn check_return_rules(
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
    rtn001(&ctx, &mut diagnostics);
    rtn002(&ctx, &mut diagnostics);

    diagnostics
}

fn rtn001(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    if ctx.host.has_yield || returns_block(ctx.semantic).is_some() || !ctx.semantic.returns().is_empty() {
        return;
    }
    if !has_other_section(ctx.semantic, BlockKind::Returns) {
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
        rule: "returns-section-missing",
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
        rule: "returns-section-extra",
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

fn returns_block(semantic: &SemanticView) -> Option<&SemanticBlock> {
    semantic.blocks().iter().find(|block| block.kind == BlockKind::Returns)
}
