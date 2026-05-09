use std::path::Path;

use pydocfix_core::{Applicability, Diagnostic, Edit, Fix};

pub(crate) struct FixOutcome {
    pub(crate) source: String,
    pub(crate) remaining_diagnostics: Vec<Diagnostic>,
}

const MAX_FIX_ITERATIONS: usize = 10;

pub(crate) fn apply_fixes_until_stable(
    source: &str,
    unsafe_fixes: bool,
    mut diagnose: impl FnMut(&str) -> Vec<Diagnostic>,
) -> Result<FixOutcome, String> {
    let mut current_source = source.to_string();
    let mut diagnostics = diagnose(&current_source);

    for _ in 0..MAX_FIX_ITERATIONS {
        let outcome = apply_applicable_fixes(&current_source, &diagnostics, unsafe_fixes)?;
        if outcome.source == current_source {
            return Ok(outcome);
        }

        current_source = outcome.source;
        diagnostics = diagnose(&current_source);
    }

    Ok(FixOutcome {
        source: current_source,
        remaining_diagnostics: diagnostics,
    })
}

pub(crate) fn apply_applicable_fixes(
    source: &str,
    diagnostics: &[Diagnostic],
    unsafe_fixes: bool,
) -> Result<FixOutcome, String> {
    let mut accepted_edits = Vec::new();
    let mut remaining_diagnostics = Vec::new();

    for diagnostic in diagnostics {
        let Some(fix) = diagnostic.fix.as_ref() else {
            remaining_diagnostics.push(diagnostic.clone());
            continue;
        };
        if !is_applicable(fix, unsafe_fixes) {
            remaining_diagnostics.push(diagnostic.clone());
            continue;
        }
        if fix_overlaps(&accepted_edits, fix) {
            remaining_diagnostics.push(diagnostic.clone());
            continue;
        }

        accepted_edits.extend(fix.edits.iter().cloned());
    }

    Ok(FixOutcome {
        source: apply_edits(source, &accepted_edits)?,
        remaining_diagnostics,
    })
}

pub(crate) fn render_diff(path: &Path, before: &str, after: &str) -> Option<String> {
    if before == after {
        return None;
    }

    let before_lines = split_lines(before);
    let after_lines = split_lines(after);
    let prefix_len = common_prefix_len(&before_lines, &after_lines);
    let suffix_len = common_suffix_len(&before_lines[prefix_len..], &after_lines[prefix_len..]);
    let before_end = before_lines.len() - suffix_len;
    let after_end = after_lines.len() - suffix_len;
    let before_hunk = &before_lines[prefix_len..before_end];
    let after_hunk = &after_lines[prefix_len..after_end];

    let mut output = Vec::new();
    output.push(format!("--- {}", path.display()));
    output.push(format!("+++ {}", path.display()));
    output.push(format!(
        "@@ -{},{} +{},{} @@",
        prefix_len + 1,
        before_hunk.len(),
        prefix_len + 1,
        after_hunk.len()
    ));
    output.extend(before_hunk.iter().map(|line| format!("-{line}")));
    output.extend(after_hunk.iter().map(|line| format!("+{line}")));
    Some(output.join("\n"))
}

fn is_applicable(fix: &Fix, unsafe_fixes: bool) -> bool {
    match fix.applicability {
        Applicability::Safe => true,
        Applicability::Unsafe => unsafe_fixes,
    }
}

fn fix_overlaps(accepted: &[Edit], candidate: &Fix) -> bool {
    candidate.edits.iter().any(|new_edit| {
        accepted.iter().any(|existing| {
            if new_edit.range.start == new_edit.range.end
                && existing.range.start == existing.range.end
                && new_edit.range.start == existing.range.start
            {
                return true;
            }
            new_edit.range.start < existing.range.end && existing.range.start < new_edit.range.end
        })
    })
}

fn apply_edits(source: &str, edits: &[Edit]) -> Result<String, String> {
    let mut sorted_edits = edits.to_vec();
    sorted_edits.sort_by_key(|edit| std::cmp::Reverse(edit.range.start));
    for pair in sorted_edits.windows(2) {
        let previous = &pair[0];
        let current = &pair[1];
        if current.range.end > previous.range.start {
            return Err(format!(
                "overlapping edits: [{}:{}] and [{}:{}]",
                current.range.start, current.range.end, previous.range.start, previous.range.end
            ));
        }
    }

    let mut bytes = source.as_bytes().to_vec();
    for edit in sorted_edits {
        if edit.range.start > edit.range.end || edit.range.end > bytes.len() {
            return Err(format!(
                "edit range [{}:{}] is outside source length {}",
                edit.range.start,
                edit.range.end,
                bytes.len()
            ));
        }
        bytes.splice(edit.range.start..edit.range.end, edit.replacement.bytes());
    }
    String::from_utf8(bytes).map_err(|error| format!("fix produced invalid UTF-8: {error}"))
}

fn split_lines(source: &str) -> Vec<&str> {
    source.split('\n').collect()
}

fn common_prefix_len<T: PartialEq>(left: &[T], right: &[T]) -> usize {
    left.iter().zip(right).take_while(|(left, right)| left == right).count()
}

fn common_suffix_len<T: PartialEq>(left: &[T], right: &[T]) -> usize {
    left.iter()
        .rev()
        .zip(right.iter().rev())
        .take_while(|(left, right)| left == right)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pydocfix_core::Range;

    fn safe_edit(start: usize, end: usize, replacement: &str) -> Fix {
        Fix {
            edits: vec![Edit {
                range: Range { start, end },
                replacement: replacement.to_string(),
            }],
            applicability: Applicability::Safe,
        }
    }

    #[test]
    fn applies_non_overlapping_fixes() {
        let diagnostics = vec![
            Diagnostic {
                rule: "A",
                message: String::new(),
                range: Range { start: 0, end: 1 },
                fix: Some(safe_edit(0, 1, "A")),
                symbol: None,
            },
            Diagnostic {
                rule: "B",
                message: String::new(),
                range: Range { start: 2, end: 3 },
                fix: Some(safe_edit(2, 3, "C")),
                symbol: None,
            },
        ];

        let outcome = apply_applicable_fixes("abc", &diagnostics, false).unwrap();

        assert_eq!(outcome.source, "AbC");
        assert!(outcome.remaining_diagnostics.is_empty());
    }

    #[test]
    fn skips_overlapping_fixes() {
        let diagnostics = vec![
            Diagnostic {
                rule: "A",
                message: String::new(),
                range: Range { start: 0, end: 2 },
                fix: Some(safe_edit(0, 2, "A")),
                symbol: None,
            },
            Diagnostic {
                rule: "B",
                message: String::new(),
                range: Range { start: 1, end: 3 },
                fix: Some(safe_edit(1, 3, "B")),
                symbol: None,
            },
        ];

        let outcome = apply_applicable_fixes("abcd", &diagnostics, false).unwrap();

        assert_eq!(outcome.source, "Acd");
        assert_eq!(outcome.remaining_diagnostics.len(), 1);
    }

    #[test]
    fn renders_changed_hunk_only() {
        let diff = render_diff(Path::new("example.py"), "a\nb\nc\n", "a\nB\nc\n").unwrap();

        assert!(diff.contains("--- example.py"));
        assert!(diff.contains("@@ -2,1 +2,1 @@"));
        assert!(diff.contains("-b"));
        assert!(diff.contains("+B"));
    }

    #[test]
    fn applies_until_stable() {
        let outcome = apply_fixes_until_stable("abc", false, |source| {
            if source == "abc" {
                vec![Diagnostic {
                    rule: "A",
                    message: String::new(),
                    range: Range { start: 1, end: 2 },
                    fix: Some(safe_edit(1, 2, "B")),
                    symbol: None,
                }]
            } else if source == "aBc" {
                vec![Diagnostic {
                    rule: "B",
                    message: String::new(),
                    range: Range { start: 2, end: 3 },
                    fix: Some(safe_edit(2, 3, "C")),
                    symbol: None,
                }]
            } else {
                Vec::new()
            }
        })
        .unwrap();

        assert_eq!(outcome.source, "aBC");
        assert!(outcome.remaining_diagnostics.is_empty());
    }
}
