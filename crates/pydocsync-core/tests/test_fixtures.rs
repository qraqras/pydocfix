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
    let rules = RuleFilter::new(vec!["arg-section".to_string()])
        .filter_diagnostics(diagnostics)
        .into_iter()
        .map(|diagnostic| diagnostic.rule.to_string())
        .collect::<Vec<_>>();

    assert_eq!(rules, vec!["arg-extra", "arg-receiver", "arg-vararg-marker"]);
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
