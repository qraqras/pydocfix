/// Static metadata for a pydocsync rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleMetadata {
    /// Rule identifier, such as `arg-missing`.
    pub code: &'static str,
}

/// Built-in rules known to the Rust engine and CLI.
pub static RULES: &[RuleMetadata] = &[
    rule("arg-section-missing"),
    rule("arg-section-extra"),
    rule("arg-receiver"),
    rule("arg-missing"),
    rule("arg-extra"),
    rule("arg-order"),
    rule("arg-duplicate"),
    rule("arg-vararg-marker"),
    rule("return-missing"),
    rule("return-extra"),
    rule("yield-missing"),
    rule("yield-extra"),
    rule("raises-section-missing"),
    rule("raises-section-extra"),
    rule("raises-missing"),
    rule("raises-extra"),
];

const fn rule(code: &'static str) -> RuleMetadata {
    RuleMetadata { code }
}

/// Return whether `code` is a built-in pydocsync rule.
pub fn is_known_rule(code: &str) -> bool {
    RULES.iter().any(|rule| rule.code == code)
}
