use std::fs;
use std::path::{Path, PathBuf};

use pydocfix_core::{ClassDocstringStyle, TypeAnnotationStyle};
use serde::Deserialize;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProjectConfig {
    pub(crate) path: Option<PathBuf>,
    pub(crate) select: Vec<String>,
    pub(crate) ignore: Vec<String>,
    pub(crate) type_annotation_style: Option<TypeAnnotationStyle>,
    pub(crate) class_docstring_style: Option<ClassDocstringStyle>,
    pub(crate) allow_optional_shorthand: bool,
    pub(crate) exclude: Vec<String>,
    pub(crate) output_format: OutputFormat,
    pub(crate) baseline: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum OutputFormat {
    #[default]
    Full,
    Concise,
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
        let Some(raw) = pyproject.tool.and_then(|tool| tool.pydocfix) else {
            return Ok(Self {
                path: Some(path),
                ..Self::default()
            });
        };

        Ok(Self {
            path: Some(path),
            select: normalize_rule_list(raw.select),
            ignore: normalize_rule_list(raw.ignore),
            type_annotation_style: raw
                .type_annotation_style
                .as_deref()
                .map(parse_type_annotation_style)
                .transpose()?,
            class_docstring_style: raw
                .class_docstring_style
                .as_deref()
                .map(parse_class_docstring_style)
                .transpose()?,
            allow_optional_shorthand: raw.allow_optional_shorthand.unwrap_or(false),
            exclude: raw.exclude.unwrap_or_default(),
            output_format: raw
                .output_format
                .as_deref()
                .map(parse_output_format)
                .transpose()?
                .unwrap_or_default(),
            baseline: raw.baseline.map(PathBuf::from),
        })
    }
}

#[derive(Debug, Default, Deserialize)]
struct PyProject {
    tool: Option<Tool>,
}

#[derive(Debug, Default, Deserialize)]
struct Tool {
    pydocfix: Option<RawConfig>,
}

#[derive(Debug, Default, Deserialize)]
struct RawConfig {
    select: Option<Vec<String>>,
    ignore: Option<Vec<String>>,
    #[serde(alias = "type-annotation-style")]
    type_annotation_style: Option<String>,
    #[serde(rename = "class-docstring-style", alias = "class_docstring_style")]
    class_docstring_style: Option<String>,
    allow_optional_shorthand: Option<bool>,
    exclude: Option<Vec<String>>,
    #[serde(rename = "output-format", alias = "output_format")]
    output_format: Option<String>,
    baseline: Option<String>,
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

pub(crate) fn parse_type_annotation_style(value: &str) -> Result<TypeAnnotationStyle, String> {
    match value {
        "signature" => Ok(TypeAnnotationStyle::Signature),
        "docstring" => Ok(TypeAnnotationStyle::Docstring),
        "both" => Ok(TypeAnnotationStyle::Both),
        _ => Err(format!(
            "invalid type_annotation_style {value:?}; expected signature, docstring, or both"
        )),
    }
}

pub(crate) fn parse_class_docstring_style(value: &str) -> Result<ClassDocstringStyle, String> {
    match value {
        "class" => Ok(ClassDocstringStyle::Class),
        "init" => Ok(ClassDocstringStyle::Init),
        "both" => Ok(ClassDocstringStyle::Both),
        _ => Err(format!(
            "invalid class-docstring-style {value:?}; expected class, init, or both"
        )),
    }
}

pub(crate) fn parse_output_format(value: &str) -> Result<OutputFormat, String> {
    match value {
        "full" => Ok(OutputFormat::Full),
        "concise" => Ok(OutputFormat::Concise),
        _ => Err(format!("invalid output-format {value:?}; expected full or concise")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_comma_separated_rule_lists() {
        assert_eq!(
            normalize_rule_list(Some(vec!["PRM, RTN".to_string(), "YLD001".to_string()])),
            vec!["PRM", "RTN", "YLD001"]
        );
    }

    #[test]
    fn parses_kebab_case_class_style() {
        let config: PyProject = toml::from_str(
            r#"
            [tool.pydocfix]
            class-docstring-style = "both"
            "#,
        )
        .unwrap();

        assert_eq!(
            config.tool.unwrap().pydocfix.unwrap().class_docstring_style,
            Some("both".to_string())
        );
        assert_eq!(parse_class_docstring_style("both"), Ok(ClassDocstringStyle::Both));
    }

    #[test]
    fn parses_kebab_case_output_format() {
        let config: PyProject = toml::from_str(
            r#"
            [tool.pydocfix]
            output-format = "concise"
            "#,
        )
        .unwrap();

        assert_eq!(
            config.tool.unwrap().pydocfix.unwrap().output_format,
            Some("concise".to_string())
        );
    }
}
