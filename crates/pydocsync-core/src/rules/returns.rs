use docstring_cst::Source;
use docstring_cst::semantic::{BlockKind, Return, SemanticBlock, SemanticView};

use crate::{AnalysisConfig, Diagnostic, DocstringHost};

use super::RuleContext;

pub(crate) fn check_return_rules(
    source: &Source,
    host: &DocstringHost,
    semantic: &SemanticView,
    _config: AnalysisConfig,
) -> Vec<Diagnostic> {
    let ctx = RuleContext { source, host, semantic };
    let mut diagnostics = Vec::new();
    rtn_entry_missing(&ctx, &mut diagnostics);
    rtn_entry_extra(&ctx, &mut diagnostics);

    diagnostics
}

fn rtn_entry_missing(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    if !has_return_signal(ctx) || !ctx.semantic.returns().is_empty() {
        return;
    }
    let Some(block) = returns_block(ctx.semantic) else {
        return;
    };
    diagnostics.push(Diagnostic {
        rule: "returns-entry-missing",
        message: "Missing return entry in Returns section.".to_string(),
        range: block.name_range.into(),
        fix: None,
        symbol: ctx.host.name.clone(),
    });
}

fn rtn_entry_extra(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    if !has_no_value_return_annotation(ctx.source, ctx.host) {
        return;
    }
    if returns_block_documents_none(ctx.source, ctx.semantic) {
        return;
    }
    diagnostics.extend(
        ctx.semantic
            .returns()
            .iter()
            .filter(|entry| !return_entry_documents_none(ctx.source, entry))
            .map(|entry| Diagnostic {
                rule: "returns-entry-extra",
                message: "Return entry is not matched by a value return or return annotation.".to_string(),
                range: entry.entry_range.into(),
                fix: None,
                symbol: ctx.host.name.clone(),
            }),
    );
}

fn has_no_value_return_annotation(source: &Source, host: &DocstringHost) -> bool {
    let Some(range) = host.return_annotation_range else {
        return false;
    };
    let Some(text) = source.slice(range.into_text_range()) else {
        return false;
    };
    let Some(annotation) = text.trim().strip_prefix("->").map(str::trim) else {
        return false;
    };
    is_no_value_annotation(annotation)
}

fn has_return_signal(ctx: &RuleContext<'_>) -> bool {
    ctx.host.has_return_value || meaningful_return_annotation(ctx.source, ctx.host).is_some()
}

fn meaningful_return_annotation<'a>(source: &'a Source, host: &DocstringHost) -> Option<&'a str> {
    let range = host.return_annotation_range?;
    let text = source.slice(range.into_text_range())?.trim();
    let annotation = text.strip_prefix("->")?.trim();
    (!annotation.is_empty() && !is_no_value_annotation(annotation)).then_some(annotation)
}

fn is_no_value_annotation(annotation: &str) -> bool {
    matches!(
        annotation,
        "None" | "NoneType" | "NoReturn" | "Never" | "typing.NoReturn" | "typing.Never"
    )
}

fn return_entry_documents_none(source: &Source, entry: &Return) -> bool {
    entry
        .type_range
        .and_then(|range| source.slice(range))
        .is_some_and(is_none_text)
        || entry
            .description_range
            .and_then(|range| source.slice(range))
            .is_some_and(is_none_text)
        || source
            .slice(entry.entry_range)
            .and_then(first_nonempty_line)
            .is_some_and(is_none_text)
}

fn first_nonempty_line(text: &str) -> Option<&str> {
    text.lines().find(|line| !line.trim().is_empty())
}

fn is_none_text(text: &str) -> bool {
    let text = text.trim().trim_end_matches('.').trim();
    matches!(text, "None" | "NoneType")
}

fn returns_block(semantic: &SemanticView) -> Option<&SemanticBlock> {
    semantic.blocks().iter().find(|block| block.kind == BlockKind::Returns)
}

fn returns_block_documents_none(source: &Source, semantic: &SemanticView) -> bool {
    let Some(block) = returns_block(semantic) else {
        return false;
    };
    let Some(text) = source.slice(block.entry_range) else {
        return false;
    };

    first_body_line_after_header(text).is_some_and(is_none_text)
}

fn first_body_line_after_header(block_text: &str) -> Option<&str> {
    let mut lines = block_text.lines();
    lines.next()?;

    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.chars().all(|ch| ch == '-') {
            continue;
        }
        return Some(trimmed);
    }

    None
}
