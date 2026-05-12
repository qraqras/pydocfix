use docstring_cst::TextRange;
use pydocsync_scanner::ByteRange;

/// File-absolute UTF-8 byte range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    /// Inclusive byte offset.
    pub start: usize,
    /// Exclusive byte offset.
    pub end: usize,
}

impl From<ByteRange> for Range {
    fn from(range: ByteRange) -> Self {
        Self {
            start: range.start(),
            end: range.end(),
        }
    }
}

impl From<TextRange> for Range {
    fn from(range: TextRange) -> Self {
        Self {
            start: range.start(),
            end: range.end(),
        }
    }
}

impl Range {
    pub(crate) fn into_text_range(self) -> TextRange {
        TextRange::new(self.start, self.end)
    }
}

/// A docstring host found in Python source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocstringHost {
    /// Host name. Modules use `None`.
    pub name: Option<String>,
    /// File-absolute range of the docstring literal, including quotes.
    pub docstring_range: Range,
    /// File-absolute range of the return annotation, including the leading `->`.
    pub return_annotation_range: Option<Range>,
    /// Whether this function has at least one value-returning `return` statement.
    pub has_return_value: bool,
    /// Whether this function contains `yield` or `yield from`.
    pub has_yield: bool,
    /// Function signature parameters in source order.
    pub signature_parameters: Vec<SignatureParameter>,
}

/// A function signature parameter used by parameter rules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignatureParameter {
    /// Display name, including `*` or `**` for varargs and kwargs.
    pub name: String,
    /// Bare name without vararg prefixes.
    pub bare_name: String,
    /// Optional type annotation text.
    pub annotation: Option<String>,
    /// Optional default value text.
    pub default_value: Option<String>,
    /// Whether this parameter is `*args`.
    pub is_vararg: bool,
    /// Whether this parameter is `**kwargs`.
    pub is_kwarg: bool,
    /// Whether this is the first method receiver (`self` or `cls`).
    pub is_implicit_receiver: bool,
}

/// Analysis options used by the Rust rule engine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnalysisConfig;

/// Rule selection applied by the core analysis engine.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuleFilter {
    /// Rule identifiers or group prefixes to disable.
    pub ignore: Vec<String>,
}

impl RuleFilter {
    /// Create a rule filter from ignored identifiers or group prefixes.
    pub fn new(ignore: Vec<String>) -> Self {
        Self { ignore }
    }

    /// Return whether a diagnostic for `rule` should be emitted.
    pub fn allows(&self, rule: &str) -> bool {
        !self.ignore.iter().any(|pattern| rule_matches(pattern, rule))
    }

    /// Filter diagnostics according to this rule filter.
    pub fn filter_diagnostics(&self, diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
        diagnostics
            .into_iter()
            .filter(|diagnostic| self.allows(diagnostic.rule))
            .collect()
    }
}

fn rule_matches(pattern: &str, rule: &str) -> bool {
    let pattern = pattern.strip_suffix('-').unwrap_or(pattern);
    rule == pattern || rule.strip_prefix(pattern).is_some_and(|suffix| suffix.starts_with('-'))
}

/// Whether a fix can be applied automatically without unsafe behavior changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Applicability {
    /// Safe fix.
    Safe,
    /// Unsafe fix.
    Unsafe,
}

/// A file-absolute text replacement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    /// Range to replace.
    pub range: Range,
    /// Replacement text.
    pub replacement: String,
}

impl Edit {
    /// Create an insertion edit at `offset`.
    pub fn insert(offset: usize, replacement: impl Into<String>) -> Self {
        Self {
            range: Range {
                start: offset,
                end: offset,
            },
            replacement: replacement.into(),
        }
    }
}

/// A set of edits that resolves a diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fix {
    /// Atomic edits.
    pub edits: Vec<Edit>,
    /// Applicability of the fix.
    pub applicability: Applicability,
}

/// A lint diagnostic emitted by the Rust rule engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// Rule code.
    pub rule: &'static str,
    /// Human-readable message.
    pub message: String,
    /// File-absolute byte range to report.
    pub range: Range,
    /// Optional fix.
    pub fix: Option<Fix>,
    /// Symbol name, when known.
    pub symbol: Option<String>,
}

/// Parsed docstring summary used by the early Rust CLI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedDocstring {
    /// Host metadata.
    pub host: DocstringHost,
    /// Parser style name.
    pub style: String,
    /// Whether the parser produced a CST for the range.
    pub parsed: bool,
    /// Number of documented parameters.
    pub parameter_count: usize,
    /// Number of documented returns entries.
    pub return_count: usize,
    /// Number of classified docstring blocks.
    pub block_count: usize,
}

/// File-level analysis output.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileReport {
    /// Parsed docstring summaries in source order.
    pub docstrings: Vec<ParsedDocstring>,
    /// Diagnostics emitted by built-in Rust rules.
    pub diagnostics: Vec<Diagnostic>,
}
