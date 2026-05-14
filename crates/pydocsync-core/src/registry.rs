/// Static metadata for a pydocsync rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleMetadata {
    /// Rule identifier, such as `args-param-missing`.
    pub code: &'static str,
}

/// Built-in rules known to the Rust engine and CLI.
pub static RULES: &[RuleMetadata] = &[
    rule("args-receiver-documented"),
    rule("args-param-missing"),
    rule("args-param-extra"),
    rule("args-param-duplicate"),
    rule("returns-entry-missing"),
    rule("returns-entry-extra"),
];

const fn rule(code: &'static str) -> RuleMetadata {
    RuleMetadata { code }
}

/// Return whether `code` is a built-in pydocsync rule.
pub fn is_known_rule(code: &str) -> bool {
    RULES.iter().any(|rule| rule.code == code)
}
