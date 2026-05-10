use docstring_cst::TextRange;
use pydocfix_scanner::ByteRange;

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

/// Kind of Python item that can own a docstring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostKind {
    /// Module-level docstring.
    Module,
    /// Class docstring.
    Class,
    /// Function, async function, or method docstring.
    Function,
}

/// A docstring host found in Python source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocstringHost {
    /// Host kind.
    pub kind: HostKind,
    /// Host name. Modules use `None`.
    pub name: Option<String>,
    /// File-absolute range of the docstring literal, including quotes.
    pub docstring_range: Range,
    /// Parent class item index from the scanner, when available.
    pub parent_index: Option<usize>,
    /// Parent class docstring range, when this host is a method inside a documented class.
    pub parent_class_docstring_range: Option<Range>,
    /// File-absolute range of the return annotation, including the leading `->`.
    pub return_annotation_range: Option<Range>,
    /// Whether this function has at least one value-returning `return` statement.
    pub has_return_value: bool,
    /// Whether this function contains `yield` or `yield from`.
    pub has_yield: bool,
    /// Raised exception records in source order.
    pub raised_exceptions: Vec<RaisedException>,
    /// Function signature parameters in source order.
    pub signature_parameters: Vec<SignatureParameter>,
}

/// A raised exception occurrence found in a function body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RaisedException {
    /// Raised exception expression text.
    pub name: String,
    /// File-absolute range of the exception expression.
    pub range: Range,
    /// Whether this record came from a bare reraise in an except handler.
    pub from_bare_except: bool,
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

/// Type annotation placement preference for rules that are disabled by default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeAnnotationStyle {
    /// Types belong in function signatures.
    Signature,
    /// Types belong in docstrings.
    Docstring,
    /// Types should appear in both signatures and docstrings.
    Both,
}

/// Preferred location for documenting class initialization details.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClassDocstringStyle {
    /// Document initializer parameters and raises in the class docstring.
    Class,
    /// Document initializer parameters and raises in the `__init__` docstring.
    Init,
    /// Allow initializer parameters and raises in either the class or `__init__` docstring.
    Both,
}

/// Analysis options used by the Rust rule engine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnalysisConfig {
    /// Optional type annotation placement preference.
    pub type_annotation_style: Option<TypeAnnotationStyle>,
    /// Optional class docstring placement preference.
    pub class_docstring_style: Option<ClassDocstringStyle>,
    /// Whether optional shorthand should be normalized when comparing types.
    pub allow_optional_shorthand: bool,
    /// Enable PRM201, which is disabled by default.
    pub enable_prm201: bool,
    /// Enable PRM202, which is disabled by default in legacy pydocfix.
    pub enable_prm202: bool,
}

/// Rule selection applied by the core analysis engine.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuleFilter {
    /// Rule selectors to enable. Empty means all default-enabled rules.
    pub select: Vec<String>,
    /// Rule selectors to disable.
    pub ignore: Vec<String>,
}

impl RuleFilter {
    /// Create a rule filter from select and ignore lists.
    pub fn new(select: Vec<String>, ignore: Vec<String>) -> Self {
        Self { select, ignore }
    }

    /// Return whether a diagnostic for `rule` should be emitted.
    pub fn allows(&self, rule: &str) -> bool {
        if !self.select.is_empty() && !self.select.iter().any(|pattern| rule_matches(pattern, rule)) {
            return false;
        }
        !self.ignore.iter().any(|pattern| rule_matches(pattern, rule))
    }

    /// Return whether a default-disabled rule was explicitly selected.
    pub fn enables(&self, rule: &str) -> bool {
        !self.select.is_empty()
            && self.select.iter().any(|pattern| rule_matches(pattern, rule))
            && !self.ignore.iter().any(|pattern| rule_matches(pattern, rule))
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
    pattern == "ALL" || rule == pattern || rule.starts_with(pattern)
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
    /// Number of documented raises entries.
    pub raise_count: usize,
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
