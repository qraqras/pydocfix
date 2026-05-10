use docstring_cst::Source;
use docstring_cst::semantic::{BlockKind, SemanticView};

use crate::{
    AnalysisConfig, Applicability, ClassDocstringStyle, Diagnostic, DocstringHost, HostKind, is_short_plain_docstring,
    missing_section_diagnostic, raises_section_stub, section_diagnostic,
};

use super::RuleContext;
use super::parameters::{args_section_stub, documentable_signature_parameters};

pub(crate) fn check_class_rules(
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
    cls001(&ctx, &mut diagnostics);
    cls101(&ctx, &mut diagnostics);
    cls102(&ctx, &mut diagnostics);
    cls103(&ctx, &mut diagnostics);
    cls104(&ctx, &mut diagnostics);
    cls105(&ctx, &mut diagnostics);
    cls106(&ctx, &mut diagnostics);
    cls201(&ctx, &mut diagnostics);
    cls202(&ctx, &mut diagnostics);
    cls203(&ctx, &mut diagnostics);
    cls204(&ctx, &mut diagnostics);
    cls205(&ctx, &mut diagnostics);
    cls206(&ctx, &mut diagnostics);

    diagnostics
}

fn cls001(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    if ctx.host.kind == HostKind::Function
        && ctx.host.name.as_deref() == Some("__init__")
        && ctx.host.parent_class_docstring_range.is_some()
        && ctx.config.class_docstring_style != Some(ClassDocstringStyle::Both)
    {
        diagnostics.push(Diagnostic {
            rule: "CLS001",
            message: "__init__ has its own docstring but the class also has a docstring.".to_string(),
            range: ctx
                .semantic
                .summary()
                .map(|summary| summary.entry_range.into())
                .unwrap_or(ctx.host.docstring_range),
            fix: None,
            symbol: ctx.host.name.clone(),
        });
    }
}

fn cls101(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    section_rules(
        ctx,
        diagnostics,
        HostKind::Class,
        None,
        BlockKind::Returns,
        "CLS101",
        "Class docstring should not have a Returns section.",
        Applicability::Safe,
    );
}

fn cls102(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    section_rules(
        ctx,
        diagnostics,
        HostKind::Class,
        None,
        BlockKind::Yields,
        "CLS102",
        "Class docstring should not have a Yields section.",
        Applicability::Safe,
    );
}

fn cls103(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    if ctx.config.class_docstring_style != Some(ClassDocstringStyle::Init) {
        return;
    }
    section_rules(
        ctx,
        diagnostics,
        HostKind::Class,
        None,
        BlockKind::Parameters,
        "CLS103",
        "Class docstring should not have an Args/Parameters section when class_docstring_style is 'init'.",
        Applicability::Unsafe,
    );
}

fn cls104(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    if ctx.config.class_docstring_style != Some(ClassDocstringStyle::Init) {
        return;
    }
    section_rules(
        ctx,
        diagnostics,
        HostKind::Class,
        None,
        BlockKind::Raises,
        "CLS104",
        "Class docstring should not have a Raises section when class_docstring_style is 'init'.",
        Applicability::Unsafe,
    );
}

fn cls105(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    if !is_short_plain_docstring(ctx.semantic)
        && ctx.host.kind == HostKind::Class
        && ctx.config.class_docstring_style == Some(ClassDocstringStyle::Class)
        && !has_block(ctx.semantic, BlockKind::Parameters)
        && !documentable_signature_parameters(ctx.host).is_empty()
    {
        diagnostics.push(missing_section_diagnostic(
            "CLS105",
            "Class docstring is missing an Args/Parameters section (class_docstring_style is 'class').",
            ctx.source,
            ctx.host,
            ctx.semantic,
            args_section_stub(ctx.source, ctx.host, ctx.semantic.style()),
        ));
    }
}

fn cls106(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    if !is_short_plain_docstring(ctx.semantic)
        && ctx.host.kind == HostKind::Class
        && ctx.config.class_docstring_style == Some(ClassDocstringStyle::Class)
        && !has_block(ctx.semantic, BlockKind::Raises)
        && !ctx.host.raised_exceptions.is_empty()
    {
        diagnostics.push(missing_section_diagnostic(
            "CLS106",
            "Class docstring is missing a Raises section (class_docstring_style is 'class').",
            ctx.source,
            ctx.host,
            ctx.semantic,
            raises_section_stub(ctx.source, ctx.host, ctx.semantic.style()),
        ));
    }
}

fn cls201(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    section_rules(
        ctx,
        diagnostics,
        HostKind::Function,
        Some("__init__"),
        BlockKind::Returns,
        "CLS201",
        "__init__ docstring should not have a Returns section.",
        Applicability::Safe,
    );
}

fn cls202(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    section_rules(
        ctx,
        diagnostics,
        HostKind::Function,
        Some("__init__"),
        BlockKind::Yields,
        "CLS202",
        "__init__ docstring should not have a Yields section.",
        Applicability::Safe,
    );
}

fn cls203(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    if ctx.config.class_docstring_style != Some(ClassDocstringStyle::Class) {
        return;
    }
    section_rules(
        ctx,
        diagnostics,
        HostKind::Function,
        Some("__init__"),
        BlockKind::Parameters,
        "CLS203",
        "__init__ docstring should not have an Args/Parameters section when class_docstring_style is 'class'.",
        Applicability::Unsafe,
    );
}

fn cls204(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    if ctx.config.class_docstring_style != Some(ClassDocstringStyle::Class) {
        return;
    }
    section_rules(
        ctx,
        diagnostics,
        HostKind::Function,
        Some("__init__"),
        BlockKind::Raises,
        "CLS204",
        "__init__ docstring should not have a Raises section when class_docstring_style is 'class'.",
        Applicability::Unsafe,
    );
}

fn cls205(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    if !is_short_plain_docstring(ctx.semantic)
        && ctx.host.kind == HostKind::Function
        && ctx.host.name.as_deref() == Some("__init__")
        && ctx.config.class_docstring_style == Some(ClassDocstringStyle::Init)
        && !has_block(ctx.semantic, BlockKind::Parameters)
        && !documentable_signature_parameters(ctx.host).is_empty()
    {
        diagnostics.push(missing_section_diagnostic(
            "CLS205",
            "__init__ docstring is missing an Args/Parameters section (class_docstring_style is 'init').",
            ctx.source,
            ctx.host,
            ctx.semantic,
            args_section_stub(ctx.source, ctx.host, ctx.semantic.style()),
        ));
    }
}

fn cls206(ctx: &RuleContext<'_>, diagnostics: &mut Vec<Diagnostic>) {
    if !is_short_plain_docstring(ctx.semantic)
        && ctx.host.kind == HostKind::Function
        && ctx.host.name.as_deref() == Some("__init__")
        && ctx.config.class_docstring_style == Some(ClassDocstringStyle::Init)
        && !has_block(ctx.semantic, BlockKind::Raises)
        && !ctx.host.raised_exceptions.is_empty()
    {
        diagnostics.push(missing_section_diagnostic(
            "CLS206",
            "__init__ docstring is missing a Raises section (class_docstring_style is 'init').",
            ctx.source,
            ctx.host,
            ctx.semantic,
            raises_section_stub(ctx.source, ctx.host, ctx.semantic.style()),
        ));
    }
}

fn section_rules(
    ctx: &RuleContext<'_>,
    diagnostics: &mut Vec<Diagnostic>,
    host_kind: HostKind,
    host_name: Option<&'static str>,
    block_kind: BlockKind,
    rule: &'static str,
    message: &'static str,
    applicability: Applicability,
) {
    diagnostics.extend(
        ctx.semantic
            .blocks()
            .iter()
            .filter(move |block| {
                ctx.host.kind == host_kind
                    && host_name.is_none_or(|name| ctx.host.name.as_deref() == Some(name))
                    && block.kind == block_kind
            })
            .map(move |block| section_diagnostic(rule, message, ctx.host, block, applicability)),
    );
}

fn has_block(semantic: &SemanticView, kind: BlockKind) -> bool {
    semantic.blocks().iter().any(|block| block.kind == kind)
}
