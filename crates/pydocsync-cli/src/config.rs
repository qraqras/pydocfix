use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProjectConfig {
    pub(crate) path: Option<PathBuf>,
    pub(crate) ignore: Vec<String>,
    pub(crate) exclude: Vec<String>,
}

impl ProjectConfig {
    pub(crate) fn load(config_path: Option<&Path>, search_roots: &[PathBuf]) -> Result<Self, String> {
        let Some(path) = config_path
            .map(Path::to_path_buf)
            .or_else(|| find_pyproject(search_roots))
        else {
            return Ok(Self::default());
        };

        let contents = fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        let pyproject: PyProject = toml::from_str(&contents).map_err(|error| format!("{}: {error}", path.display()))?;
        let Some(raw) = pyproject.tool.and_then(|tool| tool.pydocsync) else {
            return Ok(Self {
                path: Some(path),
                ..Self::default()
            });
        };

        Ok(Self {
            path: Some(path),
            ignore: normalize_rule_list(raw.ignore),
            exclude: raw.exclude.unwrap_or_default(),
        })
    }
}

#[derive(Debug, Default, Deserialize)]
struct PyProject {
    tool: Option<Tool>,
}

#[derive(Debug, Default, Deserialize)]
struct Tool {
    pydocsync: Option<RawConfig>,
}

#[derive(Debug, Default, Deserialize)]
struct RawConfig {
    ignore: Option<Vec<String>>,
    exclude: Option<Vec<String>>,
}

fn find_pyproject(search_roots: &[PathBuf]) -> Option<PathBuf> {
    search_roots
        .iter()
        .filter_map(|root| root.canonicalize().ok().or_else(|| Some(root.clone())))
        .filter_map(|root| {
            let start = if root.is_file() {
                root.parent()?.to_path_buf()
            } else {
                root
            };
            find_in_ancestors(&start)
        })
        .next()
        .or_else(|| std::env::current_dir().ok().and_then(|cwd| find_in_ancestors(&cwd)))
}

fn find_in_ancestors(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|ancestor| ancestor.join("pyproject.toml"))
        .find(|candidate| candidate.is_file())
}

fn normalize_rule_list(value: Option<Vec<String>>) -> Vec<String> {
    value
        .unwrap_or_default()
        .into_iter()
        .flat_map(|item| item.split(',').map(str::trim).map(str::to_string).collect::<Vec<_>>())
        .filter(|item| !item.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_comma_separated_rule_lists() {
        assert_eq!(
            normalize_rule_list(Some(vec![
                "args, yields".to_string(),
                "returns-section-extra".to_string()
            ])),
            vec!["args", "yields", "returns-section-extra"]
        );
    }

    #[test]
    fn parses_minimal_config() {
        let config: PyProject = toml::from_str(
            r#"
            [tool.pydocsync]
            ignore = ["yields"]
            exclude = ["build/**"]
            "#,
        )
        .unwrap();

        let raw = config.tool.unwrap().pydocsync.unwrap();
        assert_eq!(raw.ignore, Some(vec!["yields".to_string()]));
        assert_eq!(raw.exclude, Some(vec!["build/**".to_string()]));
    }
}
