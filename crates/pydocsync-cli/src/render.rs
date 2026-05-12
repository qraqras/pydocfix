use std::path::Path;

use pydocsync_core::{Applicability, Diagnostic, Range};

#[cfg(test)]
pub(crate) fn render_diagnostic(path: &Path, source: &str, diagnostic: &Diagnostic) -> String {
    DiagnosticRenderer::new(path, source).render(diagnostic)
}

pub(crate) struct DiagnosticRenderer<'a> {
    path: &'a Path,
    source: &'a str,
    line_index: LineIndex<'a>,
    colors: Colors,
}

impl<'a> DiagnosticRenderer<'a> {
    pub(crate) fn new(path: &'a Path, source: &'a str) -> Self {
        Self {
            path,
            source,
            line_index: LineIndex::new(source),
            colors: Colors::new(should_use_color()),
        }
    }

    pub(crate) fn render(&self, diagnostic: &Diagnostic) -> String {
        let range = self.line_index.position_range(diagnostic.range);
        let sep = self.colors.separator(":");
        let header = format!(
            "{}{}{}{}{}{} {}{} {}",
            self.path.display(),
            sep,
            range.start.line,
            sep,
            range.start.column,
            sep,
            self.colors.rule(diagnostic.rule),
            format!(" {}", fix_tag(diagnostic)),
            diagnostic.message,
        );

        render_full(self.source, range, header, &self.colors)
    }
}

fn fix_tag(diagnostic: &Diagnostic) -> &'static str {
    diagnostic
        .fix
        .as_ref()
        .map(|fix| match fix.applicability {
            Applicability::Safe => "[safe]",
            Applicability::Unsafe => "[unsafe]",
        })
        .unwrap_or("[]")
}

fn render_full(source: &str, range: PositionRange, header: String, colors: &Colors) -> String {
    let lines = source_lines(source);
    if range.start.line == 0 || range.start.line > lines.len() {
        return header;
    }

    let first_line = range.start.line.saturating_sub(1).max(1);
    let last_line = (range.end.line + 1).min(lines.len());
    let gutter_width = last_line.to_string().len().max(2);
    let mut output = Vec::new();
    output.push(header);
    output.push(colors.gutter(&gutter(None, gutter_width)));

    for line_number in first_line..=last_line {
        let line = lines[line_number - 1];
        output.push(format!(
            "{} {}",
            colors.gutter(&gutter(Some(line_number), gutter_width)),
            line
        ));
        if range.start.line <= line_number && line_number <= range.end.line {
            let (caret_start, caret_len) = caret_span(line, line_number, range);
            output.push(format!(
                "{} {}{}",
                colors.gutter(&gutter(None, gutter_width)),
                " ".repeat(caret_start),
                colors.caret(&"^".repeat(caret_len)),
            ));
        }
    }

    output.push(colors.gutter(&gutter(None, gutter_width)));
    output.join("\n")
}

#[derive(Clone, Copy)]
struct Colors {
    enabled: bool,
}

impl Colors {
    fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    fn paint(&self, text: &str, code: &str) -> String {
        if self.enabled {
            format!("\u{1b}[{code}m{text}\u{1b}[0m")
        } else {
            text.to_string()
        }
    }

    fn separator(&self, text: &str) -> String {
        self.paint(text, "2")
    }

    fn gutter(&self, text: &str) -> String {
        self.paint(text, "2")
    }

    fn caret(&self, text: &str) -> String {
        self.paint(text, "1;31")
    }

    fn rule(&self, text: &str) -> String {
        self.paint(text, "1;31")
    }
}

fn should_use_color() -> bool {
    #[cfg(test)]
    {
        false
    }
    #[cfg(not(test))]
    {
        use std::io::IsTerminal as _;

        if std::env::var_os("NO_COLOR").is_some() {
            return false;
        }

        if std::env::var_os("FORCE_COLOR").is_some() {
            return true;
        }

        std::io::stdout().is_terminal()
    }
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

    fn position_range(&self, range: Range) -> PositionRange {
        PositionRange {
            start: self.position(range.start),
            end: self.position(range.end.max(range.start + 1)),
        }
    }

    fn position(&self, offset: usize) -> Position {
        let offset = offset.min(self.source.len());
        let line_index = self.starts.partition_point(|start| *start <= offset).saturating_sub(1);
        let line_start = self.starts[line_index];
        let column = self
            .source
            .get(line_start..offset)
            .map(|text| text.chars().count() + 1)
            .unwrap_or_else(|| offset.saturating_sub(line_start) + 1);
        Position {
            line: line_index + 1,
            column,
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
    use pydocsync_core::Diagnostic;

    #[test]
    fn renders_header_with_line_and_column() {
        let diagnostic = Diagnostic {
            rule: "args-param-missing",
            message: "Parameter 'value' missing from docstring.".to_string(),
            range: Range { start: 16, end: 23 },
            fix: None,
            symbol: None,
        };
        let rendered = render_diagnostic(
            Path::new("example.py"),
            "def f():\n    \"\"\"Summary\"\"\"\n",
            &diagnostic,
        );

        assert!(rendered.contains("example.py:2:8: args-param-missing [] Parameter 'value' missing from docstring."));
    }

    #[test]
    fn renders_full_with_context() {
        let diagnostic = Diagnostic {
            rule: "args-param-missing",
            message: "Parameter 'value' missing from docstring.".to_string(),
            range: Range { start: 16, end: 23 },
            fix: None,
            symbol: None,
        };
        let rendered = render_diagnostic(
            Path::new("example.py"),
            "def f():\n    \"\"\"Summary\"\"\"\n",
            &diagnostic,
        );

        assert!(rendered.contains("example.py:2:8: args-param-missing [] Parameter 'value' missing from docstring."));
        assert!(rendered.contains("2 |     \"\"\"Summary\"\"\""));
        assert!(rendered.contains("|        ^^^^^^^"));
        assert!(!rendered.contains("|        ^^^^^^^ args-param-missing"));
    }

    #[test]
    fn renders_columns_by_character_not_utf8_byte() {
        let source = "def f():\n    \"\"\"説明Summary\"\"\"\n";
        let start = source.find("Summary").unwrap();
        let diagnostic = Diagnostic {
            rule: "args-param-missing",
            message: "Parameter 'value' missing from docstring.".to_string(),
            range: Range {
                start,
                end: start + "Summary".len(),
            },
            fix: None,
            symbol: None,
        };

        let rendered = render_diagnostic(Path::new("example.py"), source, &diagnostic);

        assert!(rendered.contains("example.py:2:10: args-param-missing [] Parameter 'value' missing from docstring."));
    }
}
