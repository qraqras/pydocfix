use pydocsync_core::{RuleFilter, analyze_source};

#[test]
fn fixture_rule_sets_match() {
    for fixture in ["arguments", "returns_raises"] {
        let source = fixture_source(fixture);
        let expected = fixture_rules(fixture);
        let mut actual = analyze_source(source)
            .diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.rule.to_string())
            .collect::<Vec<_>>();
        actual.sort();

        assert_eq!(actual, expected, "fixture {fixture}");
    }
}

#[test]
fn ignore_filter_supports_rule_groups() {
    let source = fixture_source("arguments");
    let diagnostics = analyze_source(source).diagnostics;
    let rules = RuleFilter::new(vec!["args-receiver".to_string()])
        .filter_diagnostics(diagnostics)
        .into_iter()
        .map(|diagnostic| diagnostic.rule.to_string())
        .collect::<Vec<_>>();

    assert!(rules.is_empty());
}

#[test]
fn numpy_grouped_parameters_match_signature_names() {
    let source = r#"def f(x, y, z):
    """Do work.

    Parameters
    ----------
    x, y : int
        Shared description.
    z : str
        Separate description.
    """
    return x + y
"#;

    let report = analyze_source(source);

    assert!(report.diagnostics.is_empty());
    assert_eq!(report.docstrings[0].parameter_count, 3);
}

#[test]
fn numpy_grouped_parameters_still_report_missing_names() {
    let source = r#"def f(x, y, z):
    """Do work.

    Parameters
    ----------
    x, y : int
        Shared description.
    """
    return x + y
"#;

    let rules = analyze_source(source)
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.rule.to_string())
        .collect::<Vec<_>>();

    assert_eq!(rules, vec!["args-param-missing"]);
}

#[test]
fn extra_parameter_reports_for_non_variadic_signature() {
    let source = r#"def f(value):
    """Do work.

    Args:
        value: Input value.
        timeout: Extra option.
    """
    return value
"#;

    let rules = analyze_source(source)
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.rule.to_string())
        .collect::<Vec<_>>();

    assert_eq!(rules, vec!["args-param-extra"]);
}

#[test]
fn extra_parameter_fix_deletes_leading_indent() {
    let source = r#"def f(value):
    """Do work.

    Args:
        value: Input value.
        timeout: Extra option.
    """
    return value
"#;

    let report = analyze_source(source);
    let diagnostic = report
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.rule == "args-param-extra")
        .expect("extra parameter diagnostic");
    let edit = only_edit(diagnostic);

    assert_eq!(edit.replacement, "");
    assert_eq!(slice(source, edit.range), "        timeout: Extra option.\n");
}

#[test]
fn receiver_fix_deletes_leading_indent() {
    let source = r#"class C:
    def f(self, value):
        """Do work.

        Args:
            self: The instance.
            value: Input value.
        """
        return value
"#;

    let report = analyze_source(source);
    let diagnostic = report
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.rule == "args-receiver-documented")
        .expect("receiver diagnostic");
    let edit = only_edit(diagnostic);

    assert_eq!(edit.replacement, "");
    assert_eq!(slice(source, edit.range), "            self: The instance.\n");
}

#[test]
fn extra_parameter_is_skipped_for_variadic_signature() {
    let source = r#"def f(value, **kwargs):
    """Do work.

    Args:
        value: Input value.
        timeout: Keyword option.
    """
    return value
"#;

    let rules = analyze_source(source)
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.rule.to_string())
        .collect::<Vec<_>>();

    assert!(rules.is_empty());
}

#[test]
fn starred_parameters_are_not_reported_missing() {
    let source = r#"def f(value, *items, **kwargs):
    """Do work.

    Args:
        value: Input value.
    """
    return value
"#;

    let rules = analyze_source(source)
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.rule.to_string())
        .collect::<Vec<_>>();

    assert!(rules.is_empty());
}

#[test]
fn extra_parameter_is_skipped_for_starargs_signature() {
    let source = r#"def f(value, *items):
    """Do work.

    Args:
        value: Input value.
        item: Extra positional item.
    """
    return value
"#;

    let rules = analyze_source(source)
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.rule.to_string())
        .collect::<Vec<_>>();

    assert!(rules.is_empty());
}

#[test]
fn malformed_parameter_like_prose_is_not_reported_extra() {
    let source = r#"def f(value):
    """Do work.

    Args:
        value: Input value.

        For "wide", we return the minimum norm solution.
    """
    return value
"#;

    let rules = analyze_source(source)
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.rule.to_string())
        .collect::<Vec<_>>();

    assert!(rules.is_empty());
}

#[test]
fn malformed_typed_parameter_text_is_not_reported_extra() {
    let source = r#"def f(value):
    """Do work.

    Args:
        value: Input value.
            prop (str): CSS property name.
    """
    return value
"#;

    let rules = analyze_source(source)
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.rule.to_string())
        .collect::<Vec<_>>();

    assert!(rules.is_empty());
}

#[test]
fn missing_return_entry_requires_existing_returns_block() {
    let source = r#"def f() -> int:
    """Do work.

    Returns:
    """
    return 1
"#;

    let rules = analyze_source(source)
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.rule.to_string())
        .collect::<Vec<_>>();

    assert_eq!(rules, vec!["returns-entry-missing"]);
}

#[test]
fn missing_return_entry_reports_same_indent_returns_body() {
    let source = r#"def f() -> int:
    """Do work.

    Returns:
    int value computed by the function.
    """
    return 1
"#;

    let rules = analyze_source(source)
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.rule.to_string())
        .collect::<Vec<_>>();

    assert_eq!(rules, vec!["returns-entry-missing"]);
}

#[test]
fn missing_return_entry_is_skipped_without_returns_block() {
    let source = r#"def f() -> int:
    """Do work."""
    return 1
"#;

    let rules = analyze_source(source)
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.rule.to_string())
        .collect::<Vec<_>>();

    assert!(rules.is_empty());
}

#[test]
fn extra_return_entry_reports_when_function_is_annotated_no_value() {
    let source = r#"def f() -> None:
    """Do work.

    Returns:
        int: A value.
    """
    return None
"#;

    let rules = analyze_source(source)
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.rule.to_string())
        .collect::<Vec<_>>();

    assert_eq!(rules, vec!["returns-entry-extra"]);
}

#[test]
fn none_return_entry_is_not_reported_extra() {
    let source = r#"def f():
    """Do work.

    Returns:
        None: No value.
    """
    return None
"#;

    let rules = analyze_source(source)
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.rule.to_string())
        .collect::<Vec<_>>();

    assert!(rules.is_empty());
}

#[test]
fn numpy_none_return_entry_is_not_reported_extra() {
    let source = r#"def f() -> None:
    """Do work.

    Returns
    -------
    None
    """
    return None
"#;

    let rules = analyze_source(source)
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.rule.to_string())
        .collect::<Vec<_>>();

    assert!(rules.is_empty());
}

#[test]
fn numpy_none_return_body_with_description_is_not_reported_extra() {
    let source = r#"def f() -> None:
    """Do work.

    Returns
    -------
    None
        This function mutates its input.
    """
    return None
"#;

    let rules = analyze_source(source)
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.rule.to_string())
        .collect::<Vec<_>>();

    assert!(rules.is_empty());
}

#[test]
fn google_none_return_body_with_description_is_not_reported_extra() {
    let source = r#"def f() -> None:
    """Do work.

    Returns:
        None
        This function mutates its input.
    """
    return None
"#;

    let rules = analyze_source(source)
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.rule.to_string())
        .collect::<Vec<_>>();

    assert!(rules.is_empty());
}

fn fixture_source(name: &str) -> &'static str {
    match name {
        "arguments" => include_str!("fixtures/arguments.py"),
        "returns_raises" => include_str!("fixtures/returns_raises.py"),
        _ => unreachable!("unknown fixture"),
    }
}

fn fixture_rules(name: &str) -> Vec<String> {
    let rules = match name {
        "arguments" => include_str!("fixtures/arguments.rules"),
        "returns_raises" => include_str!("fixtures/returns_raises.rules"),
        _ => unreachable!("unknown fixture"),
    };
    let mut rules = rules
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    rules.sort();
    rules
}

fn only_edit(diagnostic: &pydocsync_core::Diagnostic) -> &pydocsync_core::Edit {
    let fix = diagnostic.fix.as_ref().expect("diagnostic has fix");
    assert_eq!(fix.edits.len(), 1);
    &fix.edits[0]
}

fn slice(source: &str, range: pydocsync_core::Range) -> &str {
    &source[range.start..range.end]
}
