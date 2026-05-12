use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use pydocsync_scanner::{ByteRange, FileSummary, summarize_python};

#[derive(Debug, PartialEq, Eq)]
struct ExpectedSummary {
    items: Vec<ExpectedItem>,
}

#[derive(Debug, PartialEq, Eq)]
struct ExpectedItem {
    name: String,
    is_async: bool,
    is_method: bool,
    docstring_range: Option<Range>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Range {
    start: usize,
    end: usize,
}

impl From<ByteRange> for Range {
    fn from(range: ByteRange) -> Self {
        Self {
            start: range.start(),
            end: range.end(),
        }
    }
}

#[test]
fn pyscan_matches_python_ast_on_curated_corpus() {
    let corpus = [
        (
            "module_class_function",
            r#"# comment before module docs
'''Module docs.'''

class Example:
    """Class docs."""

    def method(self, value: int = 1) -> str:
        """Method docs."""
        return str(value)
"#,
        ),
        (
            "decorated_async",
            r#"class Service:
    @classmethod
    @decorator(arg="x")
    async def build(cls, value):
        """Build docs."""
        yield value
"#,
        ),
        (
            "nested_defs_and_classes",
            r#"def outer():
    """Outer docs."""
    class Inner:
        """Inner docs."""
    def nested():
        """Nested docs."""
        return 1
    return nested()
"#,
        ),
        (
            "local_function_in_control_flow",
            r#"def outer():
    """Outer docs."""
    if True:
        def hidden():
            """Hidden docs."""
            return 1
    return None
"#,
        ),
        (
            "multiline_signature",
            r#"def transform(
    value: tuple[int, str],
    callback=lambda item: item,
) -> list[str]:
    """Transform docs."""
    return [str(callback(value))]
"#,
        ),
        (
            "same_line_docstring",
            r#"def compact(): """Compact docs."""
"#,
        ),
        (
            "type_params",
            r#"class Box[T]:
    """Box docs."""

    def get[U](self, default: U) -> T | U:
        """Get docs."""
        return default
"#,
        ),
    ];

    for (name, source) in corpus {
        assert_ast_match(name, source);
    }
}

#[test]
fn pyscan_matches_python_ast_on_docstring_fixtures() {
    for path in fixture_paths() {
        let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {path:?}: {error}"));
        assert_ast_match(&path.display().to_string(), &source);
    }
}

#[test]
#[ignore = "repository corpus is broader than default unit coverage"]
fn pyscan_matches_python_ast_on_repository_corpus() {
    let paths = repository_corpus_paths();
    assert!(!paths.is_empty(), "repository corpus should not be empty");

    for path in paths {
        let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {path:?}: {error}"));
        assert_ast_match(&path.display().to_string(), &source);
    }
}

#[test]
#[ignore = "requires DOCSTRING_CST_PYSCAN_CORPUS to point at external Python projects"]
fn pyscan_matches_python_ast_on_external_corpus() {
    let paths = external_corpus_paths();
    if paths.is_empty() {
        eprintln!("set DOCSTRING_CST_PYSCAN_CORPUS to one or more Python project directories");
        return;
    }

    for path in paths {
        let source = fs::read_to_string(&path).unwrap_or_else(|error| panic!("failed to read {path:?}: {error}"));
        assert_ast_match(&path.display().to_string(), &source);
    }
}

fn assert_ast_match(name: &str, source: &str) {
    let expected = python_ast_summary(source).unwrap_or_else(|error| panic!("python ast failed for {name}: {error}"));
    let actual = actual_summary(summarize_python(source));
    assert_eq!(actual, expected, "pyscan/ast mismatch for {name}");
}

fn actual_summary(summary: FileSummary) -> ExpectedSummary {
    ExpectedSummary {
        items: summary
            .items
            .into_iter()
            .map(|function| ExpectedItem {
                name: function.name,
                is_async: function.is_async,
                is_method: function.is_method,
                docstring_range: function.docstring_range.map(Range::from),
            })
            .collect(),
    }
}

fn python_ast_summary(source: &str) -> Result<ExpectedSummary, String> {
    let mut child = Command::new("python3")
        .args(["-c", AST_SUMMARY_SCRIPT])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to spawn python3: {error}"))?;

    child
        .stdin
        .as_mut()
        .ok_or_else(|| "failed to open python stdin".to_string())?
        .write_all(source.as_bytes())
        .map_err(|error| format!("failed to write source to python: {error}"))?;

    let output = child
        .wait_with_output()
        .map_err(|error| format!("failed to wait for python: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }

    parse_ast_summary(&String::from_utf8_lossy(&output.stdout))
}

fn parse_ast_summary(output: &str) -> Result<ExpectedSummary, String> {
    let mut items = Vec::new();

    for line in output.lines() {
        let parts = line.split('\t').collect::<Vec<_>>();
        match parts.as_slice() {
            ["item", name, is_async, is_method, docstring_range] => items.push(ExpectedItem {
                name: (*name).to_string(),
                is_async: *is_async == "1",
                is_method: *is_method == "1",
                docstring_range: parse_optional_range(docstring_range)?,
            }),
            _ => return Err(format!("invalid ast summary line: {line:?}")),
        }
    }

    Ok(ExpectedSummary { items })
}

fn parse_optional_range(value: &str) -> Result<Option<Range>, String> {
    if value == "-" {
        return Ok(None);
    }
    let (start, end) = value.split_once(':').ok_or_else(|| format!("invalid range: {value}"))?;
    Ok(Some(Range {
        start: start
            .parse()
            .map_err(|error| format!("invalid range start {start}: {error}"))?,
        end: end
            .parse()
            .map_err(|error| format!("invalid range end {end}: {error}"))?,
    }))
}

fn fixture_paths() -> Vec<PathBuf> {
    let root = docstring_cst_root().join("tests/fixtures");
    let mut paths = Vec::new();
    collect_py_files(&root, &mut paths);
    paths.sort();
    paths
}

fn repository_corpus_paths() -> Vec<PathBuf> {
    let root = docstring_cst_root();
    let pydocsync_root = repository_root();
    let corpus_roots = [
        root.join("tests/fixtures"),
        root.join("python/docstring_cst"),
        root.join("python/examples"),
        root.join("python/tests"),
        pydocsync_root.join("src"),
        pydocsync_root.join("tests"),
        pydocsync_root.join("examples"),
    ];

    let mut paths = Vec::new();
    for path in corpus_roots {
        if path.exists() {
            collect_py_files(&path, &mut paths);
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("scanner crate should live under crates/ in the repository root")
        .to_path_buf()
}

fn docstring_cst_root() -> PathBuf {
    repository_root()
        .parent()
        .expect("nested pydocsync should live under the docstring-cst repository root")
        .to_path_buf()
}

fn external_corpus_paths() -> Vec<PathBuf> {
    let Some(raw_roots) = env::var_os("DOCSTRING_CST_PYSCAN_CORPUS") else {
        return Vec::new();
    };

    let mut paths = Vec::new();
    for root in env::split_paths(&raw_roots) {
        if root.exists() {
            collect_py_files(&root, &mut paths);
        }
    }
    paths.sort();
    paths.dedup();

    if let Ok(raw_limit) = env::var("DOCSTRING_CST_PYSCAN_CORPUS_LIMIT") {
        let limit = raw_limit
            .parse::<usize>()
            .unwrap_or_else(|error| panic!("invalid DOCSTRING_CST_PYSCAN_CORPUS_LIMIT={raw_limit:?}: {error}"));
        paths.truncate(limit);
    }

    paths
}

fn collect_py_files(path: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(path).unwrap_or_else(|error| panic!("failed to read {path:?}: {error}"));
    for entry in entries {
        let entry = entry.unwrap_or_else(|error| panic!("failed to read entry in {path:?}: {error}"));
        let path = entry.path();
        if should_skip_path(&path) {
            continue;
        }
        if path.is_dir() {
            collect_py_files(&path, out);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("py") {
            out.push(path);
        }
    }
}

fn should_skip_path(path: &Path) -> bool {
    path.file_name().and_then(|name| name.to_str()).is_some_and(|name| {
        matches!(
            name,
            ".git"
                | ".hg"
                | ".mypy_cache"
                | ".pytest_cache"
                | ".ruff_cache"
                | ".tox"
                | ".venv"
                | "__pycache__"
                | "build"
                | "dist"
                | "node_modules"
                | "site-packages"
                | "target"
        )
    })
}

const AST_SUMMARY_SCRIPT: &str = r#"
import ast
import sys

source_bytes = sys.stdin.buffer.read()
source = source_bytes.decode('utf-8')
line_starts = [0]
for index, byte in enumerate(source_bytes):
    if byte == 10:
        line_starts.append(index + 1)


def offset(lineno, col_offset):
    return line_starts[lineno - 1] + col_offset


def node_range(node):
    return offset(node.lineno, node.col_offset), offset(node.end_lineno, node.end_col_offset)


def range_text(value):
    if value is None:
        return '-'
    return f'{value[0]}:{value[1]}'


def docstring_range(body):
    if not body:
        return None
    first = body[0]
    if (
        isinstance(first, ast.Expr)
        and isinstance(first.value, ast.Constant)
        and isinstance(first.value.value, str)
    ):
        return node_range(first)
    return None


tree = ast.parse(source)
items = []


def walk(node, inside_class):
    if isinstance(node, ast.Module):
        for child in node.body:
            walk(child, False)
        return

    if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
        items.append((
            node.name,
            isinstance(node, ast.AsyncFunctionDef),
            inside_class,
            docstring_range(node.body),
        ))
        for child in node.body:
            walk(child, inside_class)
        return

    if isinstance(node, ast.ClassDef):
        for child in node.body:
            walk(child, True)
        return

    for child in ast.iter_child_nodes(node):
        walk(child, inside_class)


walk(tree, False)
for name, is_async, is_method, docs in items:
    print('\t'.join((
        'item',
        name,
        '1' if is_async else '0',
        '1' if is_method else '0',
        range_text(docs),
    )))
"#;
