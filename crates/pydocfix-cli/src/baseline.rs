use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use pydocfix_core::Diagnostic;
use serde::{Deserialize, Serialize};

pub(crate) type BaselineData = BTreeMap<String, Vec<BaselineEntry>>;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct BaselineEntry {
    symbol: String,
    code: String,
}

pub(crate) fn load_baseline(path: &Path) -> Result<BaselineData, String> {
    if !path.exists() {
        return Ok(BaselineData::new());
    }
    let contents = fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_str(&contents).map_err(|error| format!("{}: {error}", path.display()))
}

pub(crate) fn write_baseline(data: &BaselineData, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    let contents = serde_json::to_string_pretty(data).map_err(|error| format!("failed to encode baseline: {error}"))?;
    fs::write(path, format!("{contents}\n")).map_err(|error| format!("{}: {error}", path.display()))
}

pub(crate) fn generate_baseline(violations_by_file: &BTreeMap<String, Vec<Diagnostic>>) -> BaselineData {
    violations_by_file
        .iter()
        .filter_map(|(path, diagnostics)| {
            let entries = diagnostics
                .iter()
                .filter_map(|diagnostic| {
                    Some(BaselineEntry {
                        symbol: diagnostic.symbol.clone()?,
                        code: diagnostic.rule.to_string(),
                    })
                })
                .collect::<Vec<_>>();
            (!entries.is_empty()).then(|| (path.clone(), entries))
        })
        .collect()
}

pub(crate) fn filter_baseline_violations(
    diagnostics: Vec<Diagnostic>,
    baseline: &BaselineData,
    path: &str,
) -> Vec<Diagnostic> {
    let Some(entries) = baseline.get(path) else {
        return diagnostics;
    };
    let lookup = entries
        .iter()
        .map(|entry| (entry.symbol.as_str(), entry.code.as_str()))
        .collect::<HashSet<_>>();
    diagnostics
        .into_iter()
        .filter(|diagnostic| {
            let Some(symbol) = diagnostic.symbol.as_deref() else {
                return true;
            };
            !lookup.contains(&(symbol, diagnostic.rule))
        })
        .collect()
}

pub(crate) fn compute_updated_baseline(
    baseline: &BaselineData,
    actual_violations_by_file: &BTreeMap<String, Vec<Diagnostic>>,
) -> (bool, BaselineData) {
    let mut changed = false;
    let mut updated = BaselineData::new();

    for (path, entries) in baseline {
        let actual_pairs = actual_violations_by_file
            .get(path)
            .into_iter()
            .flatten()
            .filter_map(|diagnostic| Some((diagnostic.symbol.as_deref()?, diagnostic.rule)))
            .collect::<HashSet<_>>();
        let remaining = entries
            .iter()
            .filter(|entry| actual_pairs.contains(&(entry.symbol.as_str(), entry.code.as_str())))
            .cloned()
            .collect::<Vec<_>>();
        if remaining.len() != entries.len() {
            changed = true;
        }
        if !remaining.is_empty() {
            updated.insert(path.clone(), remaining);
        }
    }

    (changed, updated)
}

pub(crate) fn normalize_path(path: &Path, root: &Path) -> String {
    let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    canonical_path
        .strip_prefix(&canonical_root)
        .map(Path::to_path_buf)
        .unwrap_or(canonical_path)
        .to_string_lossy()
        .replace('\\', "/")
}

pub(crate) fn resolve_config_path(value: &Path, root: Option<&Path>) -> PathBuf {
    if value.is_absolute() {
        value.to_path_buf()
    } else if let Some(root) = root {
        root.join(value)
    } else {
        value.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pydocfix_core::Range;

    fn diagnostic(symbol: Option<&str>, rule: &'static str) -> Diagnostic {
        Diagnostic {
            rule,
            message: String::new(),
            range: Range { start: 0, end: 1 },
            fix: None,
            symbol: symbol.map(str::to_string),
        }
    }

    #[test]
    fn generates_symbol_rule_entries() {
        let violations = BTreeMap::from([(
            "src/example.py".to_string(),
            vec![diagnostic(Some("f"), "SUM002"), diagnostic(None, "DOC001")],
        )]);

        let baseline = generate_baseline(&violations);

        assert_eq!(baseline["src/example.py"].len(), 1);
        assert_eq!(baseline["src/example.py"][0].symbol, "f");
        assert_eq!(baseline["src/example.py"][0].code, "SUM002");
    }

    #[test]
    fn filters_matching_entries() {
        let baseline = BTreeMap::from([(
            "src/example.py".to_string(),
            vec![BaselineEntry {
                symbol: "f".to_string(),
                code: "SUM002".to_string(),
            }],
        )]);
        let diagnostics = vec![diagnostic(Some("f"), "SUM002"), diagnostic(Some("g"), "SUM002")];

        let filtered = filter_baseline_violations(diagnostics, &baseline, "src/example.py");

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].symbol.as_deref(), Some("g"));
    }
}
