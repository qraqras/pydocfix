use std::collections::HashSet;

use pydocfix_core::{Applicability, Diagnostic, Edit, Fix, ParsedDocstring, Range, is_known_rule};

#[derive(Clone, Debug, Eq, PartialEq)]
struct NoqaDirective {
    codes: Option<Vec<String>>,
    line_start: usize,
    span: Range,
}

impl NoqaDirective {
    fn suppresses(&self, rule: &str) -> bool {
        self.codes
            .as_ref()
            .is_none_or(|codes| codes.iter().any(|code| code == rule))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NoqaSuppressionResult {
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) noqa_diagnostics: Vec<Diagnostic>,
}

#[cfg(test)]
pub(crate) fn apply_noqa_suppression(
    source: &str,
    diagnostics: Vec<Diagnostic>,
    docstrings: &[ParsedDocstring],
) -> Vec<Diagnostic> {
    apply_noqa_suppression_with_report(source, diagnostics, docstrings).diagnostics
}

pub(crate) fn apply_noqa_suppression_with_report(
    source: &str,
    diagnostics: Vec<Diagnostic>,
    docstrings: &[ParsedDocstring],
) -> NoqaSuppressionResult {
    let line_index = LineIndex::new(source);
    let file_noqa = parse_file_noqa(source.lines());
    let mut inline_noqas = docstrings
        .iter()
        .filter_map(|docstring| {
            let line_info = line_index.line_at_offset(docstring.host.docstring_range.end.saturating_sub(1))?;
            let directive = parse_inline_noqa(line_info.text, line_info.start)?;
            Some(InlineNoqa {
                docstring_range: docstring.host.docstring_range,
                directive,
                used_codes: HashSet::new(),
                suppressed_any: false,
            })
        })
        .collect::<Vec<_>>();

    let mut kept = Vec::new();
    for diagnostic in diagnostics {
        if file_noqa
            .as_ref()
            .is_some_and(|directive| directive.suppresses(diagnostic.rule))
        {
            continue;
        }

        let mut suppressed = false;
        for inline_noqa in &mut inline_noqas {
            if range_contains_diagnostic(inline_noqa.docstring_range, diagnostic.range)
                && inline_noqa.directive.suppresses(diagnostic.rule)
            {
                suppressed = true;
                inline_noqa.suppressed_any = true;
                if inline_noqa.directive.codes.is_some() {
                    inline_noqa.used_codes.insert(diagnostic.rule.to_string());
                }
                break;
            }
        }
        if !suppressed {
            kept.push(diagnostic);
        }
    }

    let noqa_diagnostics = inline_noqas
        .iter()
        .flat_map(|inline_noqa| unused_noqa_diagnostics(source, inline_noqa))
        .collect();

    NoqaSuppressionResult {
        diagnostics: kept,
        noqa_diagnostics,
    }
}

fn range_contains_diagnostic(docstring_range: Range, diagnostic_range: Range) -> bool {
    docstring_range.start <= diagnostic_range.start && diagnostic_range.start <= docstring_range.end
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct InlineNoqa {
    docstring_range: Range,
    directive: NoqaDirective,
    used_codes: HashSet<String>,
    suppressed_any: bool,
}

fn unused_noqa_diagnostics(source: &str, inline_noqa: &InlineNoqa) -> Vec<Diagnostic> {
    match inline_noqa.directive.codes.as_ref() {
        None if !inline_noqa.suppressed_any => vec![unused_noqa_diagnostic(
            "Unused `noqa` directive".to_string(),
            inline_noqa.directive.span,
            Some(remove_noqa_fix(source, &inline_noqa.directive)),
        )],
        None => Vec::new(),
        Some(codes) => {
            let unused = codes
                .iter()
                .filter(|code| is_known_rule(code) && !inline_noqa.used_codes.contains(*code))
                .cloned()
                .collect::<Vec<_>>();
            if unused.is_empty() {
                return Vec::new();
            }

            let fix = Some(rewrite_noqa_fix(source, &inline_noqa.directive, codes, &unused));
            unused
                .into_iter()
                .map(|code| {
                    unused_noqa_diagnostic(
                        format!("Unused `noqa` directive for {code}"),
                        inline_noqa.directive.span,
                        fix.clone(),
                    )
                })
                .collect()
        }
    }
}

fn unused_noqa_diagnostic(message: String, range: Range, fix: Option<Fix>) -> Diagnostic {
    Diagnostic {
        rule: "NOQ001",
        message,
        range,
        fix,
        symbol: None,
    }
}

fn remove_noqa_fix(source: &str, directive: &NoqaDirective) -> Fix {
    let mut start = directive.span.start;
    while start > directive.line_start && matches!(source.as_bytes()[start - 1], b' ' | b'\t') {
        start -= 1;
    }
    Fix {
        edits: vec![Edit {
            range: Range {
                start,
                end: directive.span.end,
            },
            replacement: String::new(),
        }],
        applicability: Applicability::Safe,
    }
}

fn rewrite_noqa_fix(source: &str, directive: &NoqaDirective, codes: &[String], unused: &[String]) -> Fix {
    let unused = unused.iter().map(String::as_str).collect::<HashSet<_>>();
    let remaining = codes
        .iter()
        .filter(|code| !unused.contains(code.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if remaining.is_empty() {
        return remove_noqa_fix(source, directive);
    }

    Fix {
        edits: vec![Edit {
            range: directive.span,
            replacement: format!("# noqa: {}", remaining.join(", ")),
        }],
        applicability: Applicability::Safe,
    }
}

fn parse_file_noqa<'a>(lines: impl IntoIterator<Item = &'a str>) -> Option<NoqaDirective> {
    for line in lines {
        let trimmed = line.trim_start();
        if !starts_with_ascii_case_insensitive(trimmed, "#") {
            continue;
        }
        let after_comment = trimmed[1..].trim_start();
        if !starts_with_ascii_case_insensitive(after_comment, "pydocfix") {
            continue;
        }
        let rest = after_comment["pydocfix".len()..].trim_start();
        let Some(rest) = rest.strip_prefix(':') else {
            continue;
        };
        let rest = rest.trim_start();
        if !starts_with_ascii_case_insensitive(rest, "noqa") {
            continue;
        }
        return Some(parse_noqa_tail(&rest["noqa".len()..], 0, Range { start: 0, end: 0 }));
    }
    None
}

fn parse_inline_noqa(line: &str, line_start: usize) -> Option<NoqaDirective> {
    let comment_start = line.find('#')?;
    let comment = &line[comment_start + 1..];
    let trimmed = comment.trim_start();
    if !starts_with_ascii_case_insensitive(trimmed, "noqa") {
        return None;
    }
    let noqa_start = line_start + comment_start;
    let noqa_end = line_start + comment_start + 1 + comment_noqa_match_len(comment)?;
    Some(parse_noqa_tail(
        &trimmed["noqa".len()..],
        line_start,
        Range {
            start: noqa_start,
            end: noqa_end,
        },
    ))
}

fn comment_noqa_match_len(comment: &str) -> Option<usize> {
    let trimmed_start = comment.len() - comment.trim_start().len();
    let trimmed = comment.trim_start();
    if !starts_with_ascii_case_insensitive(trimmed, "noqa") {
        return None;
    }
    let tail = &trimmed["noqa".len()..];
    let mut len = trimmed_start + "noqa".len();
    let tail_trimmed_start = tail.len() - tail.trim_start().len();
    len += tail_trimmed_start;
    let tail = tail.trim_start();
    if let Some(codes_text) = tail.strip_prefix(':') {
        len += 1;
        len += codes_text.find('#').unwrap_or(codes_text.len());
    }
    Some(len)
}

fn parse_noqa_tail(tail: &str, line_start: usize, span: Range) -> NoqaDirective {
    let tail = tail.trim_start();
    let Some(codes_text) = tail.strip_prefix(':') else {
        return NoqaDirective {
            codes: None,
            line_start,
            span,
        };
    };
    let codes = parse_codes(codes_text);
    if codes.is_empty() {
        NoqaDirective {
            codes: None,
            line_start,
            span,
        }
    } else {
        NoqaDirective {
            codes: Some(codes),
            line_start,
            span,
        }
    }
}

fn parse_codes(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut codes = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        while index < bytes.len() && !bytes[index].is_ascii_alphabetic() {
            index += 1;
        }
        let prefix_start = index;
        while index < bytes.len() && bytes[index].is_ascii_alphabetic() {
            index += 1;
        }
        let prefix_end = index;
        let digit_start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if prefix_end > prefix_start && index - digit_start == 3 && (2..=20).contains(&(prefix_end - prefix_start)) {
            codes.push(text[prefix_start..index].to_ascii_uppercase());
        }
    }
    codes.sort();
    codes.dedup();
    codes
}

fn starts_with_ascii_case_insensitive(value: &str, prefix: &str) -> bool {
    let value = value.as_bytes();
    let prefix = prefix.as_bytes();
    value.len() >= prefix.len()
        && value
            .iter()
            .zip(prefix)
            .take(prefix.len())
            .all(|(left, right)| left.eq_ignore_ascii_case(right))
}

struct LineIndex<'a> {
    source: &'a str,
    starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    fn new(source: &'a str) -> Self {
        let mut starts = vec![0];
        for (offset, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                starts.push(offset + 1);
            }
        }
        Self { source, starts }
    }

    fn line_at_offset(&self, offset: usize) -> Option<LineInfo<'a>> {
        if offset > self.source.len() {
            return None;
        }
        let line_index = self.starts.partition_point(|start| *start <= offset).saturating_sub(1);
        let start = self.starts[line_index];
        let end = self.source[start..]
            .find('\n')
            .map(|relative| start + relative)
            .unwrap_or(self.source.len());
        Some(LineInfo {
            start,
            text: self.source[start..end]
                .strip_suffix('\r')
                .unwrap_or(&self.source[start..end]),
        })
    }
}

struct LineInfo<'a> {
    start: usize,
    text: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pydocfix_core::{DocstringHost, HostKind, ParsedDocstring};

    fn diagnostic(rule: &'static str, start: usize) -> Diagnostic {
        Diagnostic {
            rule,
            message: String::new(),
            range: Range { start, end: start + 1 },
            fix: None,
            symbol: Some("f".to_string()),
        }
    }

    fn docstring(range: Range) -> ParsedDocstring {
        ParsedDocstring {
            host: DocstringHost {
                kind: HostKind::Function,
                name: Some("f".to_string()),
                docstring_range: range,
                parent_index: None,
                parent_class_docstring_range: None,
                return_annotation_range: None,
                has_return_value: false,
                has_yield: false,
                raised_exceptions: Vec::new(),
                signature_parameters: Vec::new(),
            },
            style: "google".to_string(),
            parsed: true,
            parameter_count: 0,
            return_count: 0,
            raise_count: 0,
            block_count: 0,
        }
    }

    #[test]
    fn parses_specific_inline_codes() {
        assert_eq!(
            parse_inline_noqa("    \"\"\"  # noqa: prm001, RTN002", 0).unwrap(),
            NoqaDirective {
                codes: Some(vec!["PRM001".to_string(), "RTN002".to_string()]),
                line_start: 0,
                span: Range { start: 9, end: 31 },
            }
        );
    }

    #[test]
    fn file_level_noqa_suppresses_matching_rules() {
        let source = "# pydocfix: noqa: SUM002\ndef f():\n    \"\"\"Summary\"\"\"\n";
        let filtered = apply_noqa_suppression(source, vec![diagnostic("SUM002", 35), diagnostic("PRM001", 35)], &[]);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].rule, "PRM001");
    }

    #[test]
    fn inline_noqa_suppresses_docstring_diagnostics() {
        let source = "def f():\n    \"\"\"Summary\"\"\"  # noqa: SUM002\n";
        let start = source.find("\"\"\"").unwrap();
        let end = source.rfind("\"\"\"").unwrap() + 3;
        let filtered = apply_noqa_suppression(
            source,
            vec![diagnostic("SUM002", start + 3), diagnostic("PRM001", start + 3)],
            &[docstring(Range { start, end })],
        );

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].rule, "PRM001");
    }

    #[test]
    fn emits_noq001_for_unused_inline_codes() {
        let source = "def f():\n    \"\"\"Summary\"\"\"  # noqa: SUM002, PRM001\n";
        let start = source.find("\"\"\"").unwrap();
        let end = source.rfind("\"\"\"").unwrap() + 3;
        let report = apply_noqa_suppression_with_report(
            source,
            vec![diagnostic("SUM002", start + 3)],
            &[docstring(Range { start, end })],
        );

        assert!(report.diagnostics.is_empty());
        assert_eq!(report.noqa_diagnostics.len(), 1);
        assert_eq!(report.noqa_diagnostics[0].rule, "NOQ001");
        assert_eq!(report.noqa_diagnostics[0].message, "Unused `noqa` directive for PRM001");
        assert_eq!(
            report.noqa_diagnostics[0].fix.as_ref().unwrap().edits[0].replacement,
            "# noqa: SUM002"
        );
    }

    #[test]
    fn emits_noq001_for_unused_blanket_inline_noqa() {
        let source = "def f():\n    \"\"\"Summary.\"\"\"  # noqa\n";
        let start = source.find("\"\"\"").unwrap();
        let end = source.rfind("\"\"\"").unwrap() + 3;
        let report = apply_noqa_suppression_with_report(source, Vec::new(), &[docstring(Range { start, end })]);

        assert_eq!(report.noqa_diagnostics.len(), 1);
        assert_eq!(report.noqa_diagnostics[0].message, "Unused `noqa` directive");
        assert_eq!(
            report.noqa_diagnostics[0].fix.as_ref().unwrap().edits[0].replacement,
            ""
        );
    }

    #[test]
    fn ascii_prefix_matching_does_not_split_unicode() {
        assert!(!starts_with_ascii_case_insensitive("integer‑dtype", "pydocfix"));
        assert!(!starts_with_ascii_case_insensitive("Hence x²", "pydocfix"));
    }
}
