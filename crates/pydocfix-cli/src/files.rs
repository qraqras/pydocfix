use std::fs;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};

pub(crate) fn collect_python_files(
    paths: impl IntoIterator<Item = PathBuf>,
    exclude: &[String],
    project_root: Option<&Path>,
) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    let exclude_matcher = ExcludeMatcher::new(exclude, project_root)?;
    for path in paths {
        collect_path(&path, &mut files, &exclude_matcher)?;
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn collect_path(path: &Path, files: &mut Vec<PathBuf>, exclude: &ExcludeMatcher) -> Result<(), String> {
    if exclude.is_match(path) {
        return Ok(());
    }

    if path.is_file() {
        if is_python_file(path) {
            files.push(path.to_path_buf());
        }
        return Ok(());
    }

    if path.is_dir() {
        for entry in fs::read_dir(path).map_err(|error| format!("{}: {error}", path.display()))? {
            let entry = entry.map_err(|error| format!("{}: {error}", path.display()))?;
            let child = entry.path();
            if should_skip_dir(&child) {
                continue;
            }
            collect_path(&child, files, exclude)?;
        }
        return Ok(());
    }

    Err(format!("{} is not a file or directory", path.display()))
}

fn is_python_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("py" | "pyi")
    )
}

struct ExcludeMatcher {
    root: Option<PathBuf>,
    simple_names: Vec<String>,
    glob_set: GlobSet,
}

impl ExcludeMatcher {
    fn new(patterns: &[String], root: Option<&Path>) -> Result<Self, String> {
        let mut simple_names = Vec::new();
        let mut builder = GlobSetBuilder::new();
        for pattern in patterns
            .iter()
            .map(String::as_str)
            .map(str::trim)
            .filter(|pattern| !pattern.is_empty())
        {
            let normalized = pattern.replace('\\', "/");
            let trimmed = normalized.trim_end_matches('/');
            if !trimmed.contains(['*', '?', '[', ']']) && !trimmed.contains('/') {
                simple_names.push(trimmed.to_string());
                continue;
            }

            add_glob(&mut builder, trimmed)?;
            if normalized.ends_with('/') {
                add_glob(&mut builder, &format!("{trimmed}/**"))?;
            }
        }

        Ok(Self {
            root: root.map(Path::to_path_buf),
            simple_names,
            glob_set: builder
                .build()
                .map_err(|error| format!("invalid exclude pattern: {error}"))?,
        })
    }

    fn is_match(&self, path: &Path) -> bool {
        if self
            .simple_names
            .iter()
            .any(|name| path.file_name().and_then(|value| value.to_str()) == Some(name.as_str()))
        {
            return true;
        }

        let candidate = self
            .root
            .as_deref()
            .and_then(|root| path.strip_prefix(root).ok())
            .unwrap_or(path);
        self.glob_set.is_match(candidate)
    }
}

fn add_glob(builder: &mut GlobSetBuilder, pattern: &str) -> Result<(), String> {
    builder.add(Glob::new(pattern).map_err(|error| format!("invalid exclude pattern {pattern:?}: {error}"))?);
    Ok(())
}

fn should_skip_dir(path: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(".git" | ".venv" | "__pycache__" | "target")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_name_exclude_matches_leaf_name() {
        let matcher = ExcludeMatcher::new(&["build".to_string()], None).unwrap();

        assert!(matcher.is_match(Path::new("src/build")));
        assert!(!matcher.is_match(Path::new("src/build_script.py")));
    }

    #[test]
    fn glob_exclude_matches_relative_to_root() {
        let matcher = ExcludeMatcher::new(&["tests/**/fixtures/".to_string()], Some(Path::new("/project"))).unwrap();

        assert!(matcher.is_match(Path::new("/project/tests/unit/fixtures/example.py")));
        assert!(!matcher.is_match(Path::new("/project/tests/unit/example.py")));
    }

    #[test]
    fn accepts_python_source_and_stub_files() {
        assert!(is_python_file(Path::new("example.py")));
        assert!(is_python_file(Path::new("example.pyi")));
        assert!(!is_python_file(Path::new("example.pyc")));
    }
}
