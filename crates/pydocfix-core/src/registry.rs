/// Static metadata for a pydocfix rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleMetadata {
    /// Rule code, such as `PRM001`.
    pub code: &'static str,
    /// Whether the rule is enabled without an explicit selector.
    pub default_enabled: bool,
}

/// Built-in rules known to the Rust engine and CLI.
pub static RULES: &[RuleMetadata] = &[
    rule("SUM001"),
    rule("SUM002"),
    rule("PRM001"),
    rule("PRM002"),
    rule("PRM003"),
    rule("PRM004"),
    rule("PRM005"),
    rule("PRM006"),
    rule("PRM007"),
    rule("PRM008"),
    rule("PRM009"),
    rule("PRM101"),
    rule("PRM102"),
    rule("PRM103"),
    rule("PRM104"),
    rule("PRM105"),
    rule("PRM106"),
    disabled_rule("PRM201"),
    disabled_rule("PRM202"),
    rule("RTN001"),
    rule("RTN002"),
    rule("RTN003"),
    rule("RTN101"),
    rule("RTN102"),
    rule("RTN103"),
    rule("RTN104"),
    rule("RTN105"),
    rule("RTN106"),
    rule("YLD001"),
    rule("YLD002"),
    rule("YLD003"),
    rule("YLD101"),
    rule("YLD102"),
    rule("YLD103"),
    rule("YLD104"),
    rule("YLD105"),
    rule("YLD106"),
    rule("RIS001"),
    rule("RIS002"),
    rule("RIS003"),
    rule("RIS004"),
    rule("RIS005"),
    rule("DOC001"),
    rule("DOC002"),
    rule("DOC003"),
    rule("CLS001"),
    rule("CLS101"),
    rule("CLS102"),
    rule("CLS103"),
    rule("CLS104"),
    rule("CLS105"),
    rule("CLS106"),
    rule("CLS201"),
    rule("CLS202"),
    rule("CLS203"),
    rule("CLS204"),
    rule("CLS205"),
    rule("CLS206"),
    rule("NOQ001"),
];

const fn rule(code: &'static str) -> RuleMetadata {
    RuleMetadata {
        code,
        default_enabled: true,
    }
}

const fn disabled_rule(code: &'static str) -> RuleMetadata {
    RuleMetadata {
        code,
        default_enabled: false,
    }
}

/// Return whether `code` is a built-in pydocfix rule.
pub fn is_known_rule(code: &str) -> bool {
    RULES.iter().any(|rule| rule.code == code)
}

/// Return whether `code` is known but disabled unless explicitly selected.
pub fn is_default_disabled_rule(code: &str) -> bool {
    RULES.iter().any(|rule| rule.code == code && !rule.default_enabled)
}
