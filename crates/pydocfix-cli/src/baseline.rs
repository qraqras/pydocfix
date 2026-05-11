use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use pydocfix_core::Diagnostic;
use serde::{Deserialize, Serialize};

pub(crate) type BaselineData = BTreeMap<String, Vec<BaselineEntry>>;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct BaselineEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    symbol: Option<String>,
    code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    start: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    end: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    message_hash: Option<u64>,
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
                .map(BaselineEntry::from_diagnostic)
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
    diagnostics
        .into_iter()
        .filter(|diagnostic| !entries.iter().any(|entry| entry.matches(diagnostic)))
        .collect()
}

pub(crate) fn compute_updated_baseline(
    baseline: &BaselineData,
    actual_violations_by_file: &BTreeMap<String, Vec<Diagnostic>>,
) -> (bool, BaselineData) {
    let mut changed = false;
    let mut updated = BaselineData::new();

    for (path, entries) in baseline {
        let empty = Vec::new();
        let actual = actual_violations_by_file.get(path).unwrap_or(&empty);
        let remaining = entries
            .iter()
            .filter(|entry| actual.iter().any(|diagnostic| entry.matches(diagnostic)))
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

impl BaselineEntry {
    fn from_diagnostic(diagnostic: &Diagnostic) -> Self {
        Self {
            symbol: diagnostic.symbol.clone(),
            code: diagnostic.rule.to_string(),
            start: Some(diagnostic.range.start),
            end: Some(diagnostic.range.end),
            message_hash: Some(stable_hash(&diagnostic.message)),
        }
    }

    fn matches(&self, diagnostic: &Diagnostic) -> bool {
        if self.code != diagnostic.rule || self.symbol.as_deref() != diagnostic.symbol.as_deref() {
            return false;
        }

        match (self.start, self.end, self.message_hash) {
            (Some(start), Some(end), Some(message_hash)) => {
                start == diagnostic.range.start
                    && end == diagnostic.range.end
                    && message_hash == stable_hash(&diagnostic.message)
            }
            _ => true,
        }
    }
}

fn stable_hash(value: &str) -> u64 {
    value.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
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

        assert_eq!(baseline["src/example.py"].len(), 2);
        assert_eq!(baseline["src/example.py"][0].symbol.as_deref(), Some("f"));
        assert_eq!(baseline["src/example.py"][0].code, "SUM002");
        assert_eq!(baseline["src/example.py"][1].symbol, None);
        assert_eq!(baseline["src/example.py"][1].code, "DOC001");
    }

    #[test]
    fn filters_matching_entries() {
        let baseline = BTreeMap::from([(
            "src/example.py".to_string(),
            vec![BaselineEntry {
                symbol: Some("f".to_string()),
                code: "SUM002".to_string(),
                start: Some(0),
                end: Some(1),
                message_hash: Some(stable_hash("")),
            }],
        )]);
        let diagnostics = vec![diagnostic(Some("f"), "SUM002"), diagnostic(Some("g"), "SUM002")];

        let filtered = filter_baseline_violations(diagnostics, &baseline, "src/example.py");

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].symbol.as_deref(), Some("g"));
    }

    #[test]
    fn fingerprint_prevents_symbol_rule_collisions() {
        let baseline = BTreeMap::from([(
            "src/example.py".to_string(),
            vec![BaselineEntry {
                symbol: Some("f".to_string()),
                code: "SUM002".to_string(),
                start: Some(0),
                end: Some(1),
                message_hash: Some(stable_hash("")),
            }],
        )]);
        let diagnostics = vec![Diagnostic {
            range: Range { start: 10, end: 11 },
            ..diagnostic(Some("f"), "SUM002")
        }];

        let filtered = filter_baseline_violations(diagnostics, &baseline, "src/example.py");

        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn legacy_symbol_rule_entries_still_match() {
        let baseline = BTreeMap::from([(
            "src/example.py".to_string(),
            vec![BaselineEntry {
                symbol: Some("f".to_string()),
                code: "SUM002".to_string(),
                start: None,
                end: None,
                message_hash: None,
            }],
        )]);
        let diagnostics = vec![diagnostic(Some("f"), "SUM002")];

        let filtered = filter_baseline_violations(diagnostics, &baseline, "src/example.py");

        assert!(filtered.is_empty());
    }
}
