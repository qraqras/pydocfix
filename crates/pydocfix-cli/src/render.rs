use std::path::Path;

use pydocfix_core::{Applicability, Diagnostic, Range};

use crate::config::OutputFormat;

#[cfg(test)]
pub(crate) fn render_diagnostic(path: &Path, source: &str, diagnostic: &Diagnostic, format: OutputFormat) -> String {
    DiagnosticRenderer::new(path, source, format).render(diagnostic)
}

pub(crate) struct DiagnosticRenderer<'a> {
    path: &'a Path,
    source: &'a str,
    format: OutputFormat,
    line_index: LineIndex,
}

impl<'a> DiagnosticRenderer<'a> {
    pub(crate) fn new(path: &'a Path, source: &'a str, format: OutputFormat) -> Self {
        Self {
            path,
            source,
            format,
            line_index: LineIndex::new(source),
        }
    }

    pub(crate) fn render(&self, diagnostic: &Diagnostic) -> String {
        let range = self.line_index.position_range(diagnostic.range);
        let header = format!(
            "{}:{}:{}: {}{} {}",
            self.path.display(),
            range.start.line,
            range.start.column,
            diagnostic.rule,
            fix_tag(diagnostic),
            diagnostic.message,
        );

        if self.format == OutputFormat::Concise {
            return header;
        }

        render_full(self.source, diagnostic.rule, range, header)
    }
}

fn fix_tag(diagnostic: &Diagnostic) -> &'static str {
    diagnostic
        .fix
        .as_ref()
        .map(|fix| match fix.applicability {
            Applicability::Safe => " [safe]",
            Applicability::Unsafe => " [unsafe]",
        })
        .unwrap_or("")
}

fn render_full(source: &str, rule: &str, range: PositionRange, header: String) -> String {
    let lines = source_lines(source);
    if range.start.line == 0 || range.start.line > lines.len() {
        return header;
    }

    let first_line = range.start.line.saturating_sub(1).max(1);
    let last_line = (range.end.line + 1).min(lines.len());
    let gutter_width = last_line.to_string().len().max(2);
    let mut output = Vec::new();
    output.push(header);
    output.push(gutter(None, gutter_width));

    for line_number in first_line..=last_line {
        let line = lines[line_number - 1];
        output.push(format!("{} {}", gutter(Some(line_number), gutter_width), line));
        if range.start.line <= line_number && line_number <= range.end.line {
            let (caret_start, caret_len) = caret_span(line, line_number, range);
            let suffix = if line_number == range.start.line {
                format!(" {rule}")
            } else {
                String::new()
            };
            output.push(format!(
                "{} {}{}{}",
                gutter(None, gutter_width),
                " ".repeat(caret_start),
                "^".repeat(caret_len),
                suffix,
            ));
        }
    }

    output.push(gutter(None, gutter_width));
    output.join("\n")
}

fn gutter(line_number: Option<usize>, width: usize) -> String {
    match line_number {
        Some(line_number) => format!("{line_number:>width$} |"),
        None => format!("{:>width$} |", ""),
    }
}

fn caret_span(line: &str, line_number: usize, range: PositionRange) -> (usize, usize) {
    let line_width = line.chars().count();
    let start = if line_number == range.start.line {
        range.start.column.saturating_sub(1)
    } else {
        0
    };
    let end = if line_number == range.end.line {
        range.end.column.saturating_sub(1)
    } else {
        line_width.max(start + 1)
    };
    (start, end.saturating_sub(start).max(1))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Position {
    line: usize,
    column: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PositionRange {
    start: Position,
    end: Position,
}

struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    fn new(source: &str) -> Self {
        let mut starts = vec![0];
        for (offset, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                starts.push(offset + 1);
            }
        }
        Self { starts }
    }

    fn position_range(&self, range: Range) -> PositionRange {
        PositionRange {
            start: self.position(range.start),
            end: self.position(range.end.max(range.start + 1)),
        }
    }

    fn position(&self, offset: usize) -> Position {
        let line_index = self.starts.partition_point(|start| *start <= offset).saturating_sub(1);
        Position {
            line: line_index + 1,
            column: offset.saturating_sub(self.starts[line_index]) + 1,
        }
    }
}

fn source_lines(source: &str) -> Vec<&str> {
    source
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pydocfix_core::Diagnostic;

    #[test]
    fn renders_concise_with_line_and_column() {
        let diagnostic = Diagnostic {
            rule: "SUM002",
            message: "Summary doesn't end with period.".to_string(),
            range: Range { start: 16, end: 23 },
            fix: None,
            symbol: None,
        };
        let rendered = render_diagnostic(
            Path::new("example.py"),
            "def f():\n    \"\"\"Summary\"\"\"\n",
            &diagnostic,
            OutputFormat::Concise,
        );

        assert_eq!(rendered, "example.py:2:8: SUM002 Summary doesn't end with period.");
    }

    #[test]
    fn renders_full_with_context() {
        let diagnostic = Diagnostic {
            rule: "SUM002",
            message: "Summary doesn't end with period.".to_string(),
            range: Range { start: 16, end: 23 },
            fix: None,
            symbol: None,
        };
        let rendered = render_diagnostic(
            Path::new("example.py"),
            "def f():\n    \"\"\"Summary\"\"\"\n",
            &diagnostic,
            OutputFormat::Full,
        );

        assert!(rendered.contains("example.py:2:8: SUM002 Summary doesn't end with period."));
        assert!(rendered.contains("2 |     \"\"\"Summary\"\"\""));
        assert!(rendered.contains("|        ^^^^^^^ SUM002"));
    }
}
