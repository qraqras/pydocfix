use docstring_cst::semantic::{BlockKind, SemanticBlock, SemanticView};
use docstring_cst::{DocstringStyle, Source, TextRange};

use crate::{AnalysisConfig, Applicability, Diagnostic, DocstringHost, Edit, Fix, HostKind, Range};

use super::RuleContext;

pub(crate) fn check_doc_rules(
    source: &Source,
    host: &DocstringHost,
    semantic: &SemanticView,
    config: AnalysisConfig,
) -> Vec<Diagnostic> {
    let ctx = RuleContext {
        source,
        host,
        semantic,
        config,
    };
    let mut diagnostics = Vec::new();
    doc001(&ctx, &mut diagnostics);
    doc002(&ctx, &mut diagnostics);
    doc003(&ctx, &mut diagnostics);

    diagnostics
}

fn doc001(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    if !matches!(ctx.host.kind, HostKind::Function | HostKind::Class) {
        return;
    }
    if !matches!(ctx.semantic.style(), DocstringStyle::Google | DocstringStyle::Numpy) {
        return;
    }

    let blocks = ctx.semantic.blocks();
    if blocks.len() < 2 {
        return;
    }

    let sorted_indices = sorted_block_indices(blocks);
    if sorted_indices.iter().copied().eq(0..blocks.len()) {
        return;
    }

    let first_wrong_index = sorted_indices
        .iter()
        .enumerate()
        .find_map(|(index, sorted_index)| (index != *sorted_index).then_some(index))
        .unwrap_or(0);
    let range = blocks[first_wrong_index].entry_range.into();

    let Some(first_block) = blocks.first() else {
        return;
    };
    let Some(last_block) = blocks.last() else {
        return;
    };
    let start = first_block.entry_range.start();
    let end = last_block.entry_range.end();
    let Some(original) = ctx.source.source().get(start..end) else {
        return;
    };
    let mut replacement = String::new();
    for (position, block_index) in sorted_indices.iter().enumerate() {
        let block = blocks[*block_index];
        let Some(block_text) = ctx.source.slice(block.entry_range) else {
            return;
        };
        replacement.push_str(block_text);
        if position + 1 < sorted_indices.len() {
            let gap_start = blocks[position].entry_range.end();
            let gap_end = blocks[position + 1].entry_range.start();
            replacement.push_str(ctx.source.source().get(gap_start..gap_end).unwrap_or(""));
        }
    }
    if replacement == original {
        return;
    }

    diagnostics.push(Diagnostic {
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
        symbol: ctx.host.name.clone(),
    });
}

fn doc002(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let is_numpy = ctx.semantic.style() == DocstringStyle::Numpy;
    for entry_range in doc_entry_ranges(ctx.semantic) {
        let Some(block) = ctx.semantic.blocks().iter().find(|block| {
            block.entry_range.start() <= entry_range.start() && entry_range.start() < block.entry_range.end()
        }) else {
            continue;
        };
        let line_start = ctx.source.source()[..entry_range.start()]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        let entry_start = first_non_ws_on_line(ctx.source.source(), entry_range.start(), entry_range.end());
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
            symbol: ctx.host.name.clone(),
        });
    }
}

fn doc003(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    if !matches!(ctx.host.kind, HostKind::Module | HostKind::Class | HostKind::Function) {
        return;
    }
    if ctx.semantic.style() != DocstringStyle::Plain {
        return;
    }
    if ctx.semantic.extended_summary().is_some() || !ctx.semantic.blocks().is_empty() {
        return;
    }
    let Some(summary) = ctx.semantic.summary() else {
        return;
    };
    let Some(open_quote) = ctx.semantic.open_quote() else {
        return;
    };
    let Some(close_quote) = ctx.semantic.close_quote() else {
        return;
    };
    let Some(body) = ctx
        .source
        .source()
        .get(open_quote.entry_range.end()..close_quote.entry_range.start())
    else {
        return;
    };
    if !body.contains('\n') {
        return;
    }
    let non_empty_lines = body.lines().filter(|line| !line.trim().is_empty()).count();
    if non_empty_lines != 1 {
        return;
    }
    let Some(summary_text) = ctx.source.slice(summary.entry_range).map(str::trim) else {
        return;
    };
    if summary_text.is_empty() {
        return;
    }
    let Some(open_text) = ctx.source.slice(open_quote.entry_range) else {
        return;
    };
    let Some(close_text) = ctx.source.slice(close_quote.entry_range) else {
        return;
    };
    diagnostics.push(Diagnostic {
        rule: "DOC003",
        message: "One-line docstring should be written on a single line.".to_string(),
        range: summary.entry_range.into(),
        fix: Some(Fix {
            edits: vec![Edit {
                range: ctx.host.docstring_range,
                replacement: format!("{open_text}{summary_text}{close_text}"),
            }],
            applicability: Applicability::Safe,
        }),
        symbol: ctx.host.name.clone(),
    });
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

fn first_non_ws_on_line(source: &str, start: usize, end: usize) -> usize {
    let bytes = source.as_bytes();
    let mut offset = start;
    while offset < end && matches!(bytes[offset], b' ' | b'\t') {
        offset += 1;
    }
    offset
}

fn doc_entry_ranges(semantic: &SemanticView) -> Vec<TextRange> {
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
