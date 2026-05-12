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
    let rules = RuleFilter::new(vec!["args-section".to_string()])
        .filter_diagnostics(diagnostics)
        .into_iter()
        .map(|diagnostic| diagnostic.rule.to_string())
        .collect::<Vec<_>>();

    assert_eq!(
        rules,
        vec![
            "args-param-extra",
            "args-receiver-documented",
            "args-vararg-marker-missing"
        ]
    );
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
