use docstring_cst::Source;
use docstring_cst::semantic::{BlockKind, Raise, SemanticBlock, SemanticView};

use crate::{
    AnalysisConfig, Applicability, Diagnostic, DocstringHost, Edit, Fix, HostKind, bare_exception_name,
    raises_entry_append_text, raises_section_stub, unique_raised_exception_names,
};

use super::{RuleContext, has_other_section};

pub(crate) fn check_raise_rules(
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
    ris001(&ctx, &mut diagnostics);
    ris002(&ctx, &mut diagnostics);
    ris004(&ctx, &mut diagnostics);
    ris005(&ctx, &mut diagnostics);

    diagnostics
}

fn ris001(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let raises_block = raises_block(ctx.semantic);
    if ctx.host.name.as_deref() == Some("__init__")
        || ctx.host.raised_exceptions.is_empty()
        || raises_block.is_some()
        || !ctx.semantic.raises().is_empty()
        || !has_other_section(ctx.semantic, BlockKind::Raises)
    {
        return;
    }
    let insert_offset = ctx
        .semantic
        .close_quote()
        .map(|quote| quote.entry_range.start())
        .unwrap_or(ctx.host.docstring_range.end);
    diagnostics.push(Diagnostic {
        rule: "raises-section-missing",
        message: "Missing Raises section in docstring.".to_string(),
        range: ctx
            .semantic
            .summary()
            .map(|summary| summary.entry_range.into())
            .unwrap_or(ctx.host.docstring_range),
        fix: Some(Fix {
            edits: vec![Edit::insert(
                insert_offset,
                raises_section_stub(ctx.source, ctx.host, ctx.semantic.style()),
            )],
            applicability: Applicability::Unsafe,
        }),
        symbol: ctx.host.name.clone(),
    });
}

fn ris002(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    if !ctx.host.raised_exceptions.is_empty() {
        return;
    }
    let Some(block) = raises_block(ctx.semantic) else {
        return;
    };
    diagnostics.push(Diagnostic {
        rule: "raises-section-extra",
        message: "Unnecessary Raises section in docstring.".to_string(),
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

fn ris004(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let Some(block) = raises_block(ctx.semantic) else {
        return;
    };
    let documented_names: Vec<&str> = ctx
        .semantic
        .raises()
        .iter()
        .filter_map(|entry| entry.exception_range.and_then(|range| ctx.source.slice(range)))
        .map(bare_exception_name)
        .collect();

    diagnostics.extend(
        unique_raised_exception_names(ctx.host)
            .into_iter()
            .filter(|raised_exception| {
                !documented_names
                    .iter()
                    .any(|documented| *documented == *raised_exception)
            })
            .map(|raised_exception| Diagnostic {
                rule: "raises-exception-missing",
                message: format!("Raised exception '{raised_exception}' not documented in Raises section."),
                range: block.name_range.into(),
                fix: Some(Fix {
                    edits: vec![Edit::insert(
                        block.entry_range.end(),
                        raises_entry_append_text(ctx.source, block.name_range, ctx.semantic.style(), raised_exception),
                    )],
                    applicability: Applicability::Unsafe,
                }),
                symbol: ctx.host.name.clone(),
            }),
    );
}

fn ris005(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    for raise_entry in ctx.semantic.raises() {
        ris005_entry(ctx, diagnostics, raise_entry);
    }
}

fn ris005_entry(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>, raise_entry: &Raise) {
    let Some(exception_range) = raise_entry.exception_range else {
        return;
    };
    let Some(documented_name) = ctx.source.slice(exception_range) else {
        return;
    };
    let bare_documented_name = bare_exception_name(documented_name);
    if unique_raised_exception_names(ctx.host)
        .iter()
        .any(|raised_name| *raised_name == bare_documented_name)
    {
        return;
    }
    diagnostics.push(Diagnostic {
        rule: "raises-exception-extra",
        message: format!("Raises entry '{documented_name}' not raised in function body."),
        range: exception_range.into(),
        fix: Some(Fix {
            edits: vec![Edit {
                range: raise_entry.entry_range.into(),
                replacement: String::new(),
            }],
            applicability: Applicability::Unsafe,
        }),
        symbol: ctx.host.name.clone(),
    });
}

fn raises_block(semantic: &SemanticView) -> Option<&SemanticBlock> {
    semantic.blocks().iter().find(|block| block.kind == BlockKind::Raises)
}
