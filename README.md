# pydocfix

[![PyPI - Version](https://img.shields.io/pypi/v/pydocfix?color=0062A8)](https://pypi.org/project/pydocfix/)
[![PyPI - Python Version](https://img.shields.io/pypi/pyversions/pydocfix?color=0062A8)](https://devguide.python.org/versions/)

A Rust-powered Python docstring linter that checks **signature ↔ docstring consistency** and **auto-fixes** violations.

Inspired by [pydoclint](https://github.com/jsh9/pydoclint), pydocfix goes further by **automatically repairing** the issues it finds.

> [!NOTE]
> This project is in **beta** (v0.1.0b2).
> APIs and behavior may change before the stable release.

## Why pydocfix?

[pydoclint](https://github.com/jsh9/pydoclint) pioneered fast signature ↔ docstring consistency checking for Python. However, it can only *report* violations — all corrections must be done by hand.

pydocfix is implemented in Rust and built on docstring CST parsing. CST preserves every token's byte offset, whitespace, and formatting, enabling:

- **Byte-level diagnostics** — point to the exact token (parameter name, type annotation, section header), not just the line
- **Surgical auto-fix** — edits replace precise byte ranges, so fixes never corrupt adjacent content
- **Iterative fix loop** — apply non-overlapping fixes, re-parse, repeat until stable

### Example

pydocfix points to the **exact token** — the 3-char `int` inside `price (int)`:

<img alt="pydocfix diagnostic output" src="docs/images/diagnostic.gif">

Run `--diff` to preview the surgical fix — only `int` → `float` is changed:

<img alt="pydocfix diff output" src="docs/images/diff.gif">

Apply with `pydocfix check example.py --fix --unsafe-fixes`.

## Features

- **Auto-fix** — Automatically repair docstring issues with safe/unsafe classification
- **Many rules** across multiple categories (Summary, Parameters, Returns, Yields, Raises, Docstring)
- **Google & NumPy style** support powered by Rust CST parsing
- **Signature ↔ docstring consistency** — type mismatches, missing/extra parameters, ordering
- **Default value checking** — detect missing `optional` / `default` annotations
- **Precise diagnostics** — byte-level position information for every violation
- **Baseline** — suppress existing violations so only new ones are reported
- **noqa** — suppress specific violations inline or file-wide

## Benchmark

These benchmarks use the same corpus selection as the historical pydocfix benchmark: shallow-clone each OSS project, then scan the main package directory (`numpy/` or `sklearn/`) rather than the whole repository.
`pydocfix` performs linting and fix generation in one pass; `pydoclint` is lint-only.

### Parallel (default worker pool — 10-core dev container)

| Corpus | Files | Lines | pydocfix | pydoclint | Speedup |
|--------|------:|------:|---------:|----------:|--------:|
| [numpy](https://github.com/numpy/numpy) | 425 | 252K | 0.039 sec | 4.97 sec | **126.4x** |
| [scikit-learn](https://github.com/scikit-learn/scikit-learn) | 637 | 373K | 0.041 sec | 7.31 sec | **177.5x** |

### Single-threaded (`--jobs 1`)

| Corpus | Files | Lines | pydocfix | pydoclint | Speedup |
|--------|------:|------:|---------:|----------:|--------:|
| [numpy](https://github.com/numpy/numpy) | 425 | 252K | 0.251 sec | 4.97 sec | **19.8x** |
| [scikit-learn](https://github.com/scikit-learn/scikit-learn) | 637 | 373K | 0.278 sec | 7.31 sec | **26.3x** |

| Corpus | pydocfix violations | pydoclint violations |
|--------|--------------------:|---------------------:|
| [numpy](https://github.com/numpy/numpy) | 3,466 | 2,787 |
| [scikit-learn](https://github.com/scikit-learn/scikit-learn) | 6,498 | 5,238 |

Measured on May 9, 2026 with `hyperfine --warmup 1 --runs 5` in the project dev container.
`pydocfix` was built with `cargo build --release` and run with `--output-format concise`; `pydoclint` was version 0.8.3 and was run with `--style=numpy --arg-type-hints-in-signature=False --arg-type-hints-in-docstring=False`.
Violation counts are not expected to match exactly because the tools expose different rule sets and parsing behavior.
The benchmark can be reproduced with `python benchmarks/bench.py --target numpy,scikit-learn --docstyle numpy`.

## Feature comparison

|  | pydocfix | pydoclint |
|--|:--------:|:---------:|
| Auto-fix (safe + unsafe) | ✅ | — |
| Google style | ✅ | ✅ |
| NumPy style | ✅ | ✅ |
| Sphinx style | — | ✅ |
| Parameter checking | ✅ | ✅ |
| Return type checking | ✅ | ✅ |
| Yield checking | ✅ | ✅ |
| Raises checking | ✅ | ✅ |
| Class docstring / `__init__` rules | ✅ | ✅ |
| Class attribute checking | - | ✅ |
| Default value checking (`optional` / `default`) | ✅ | — |
| Byte-level diagnostics | ✅ | — |
| Baseline suppression | ✅ | ✅ |
| Inline `# noqa` | ✅ | ✅ |
| flake8 plugin | — | ✅ |
| pre-commit hook | ✅ | ✅ |
| Parallel execution | ✅ | — |

## Installation

```bash
pip install pydocfix
```

Requires Python 3.11+.

The PyPI package ships a native Rust `pydocfix` executable, similar to tools like Ruff. It does not require a Python runtime dependency such as `click` or a Python docstring parser package at execution time.

For local packaging work, build the wheel through `uv`:

```bash
uv run maturin build --release --out dist
```

## Quick Start

```bash
# Check docstrings (report only)
pydocfix check src/

# Show diff of proposed fixes
pydocfix check src/ --diff

# Apply safe fixes
pydocfix check src/ --fix

# Apply safe + unsafe fixes
pydocfix check src/ --fix --unsafe-fixes

# Select / ignore specific rules or categories
pydocfix check src/ --select PRM --ignore RTN,YLD

# Override parallelism
pydocfix check src/ --jobs 4

# Concise (single-line) output
pydocfix check src/ --output-format concise
```

## Configuration

Configure via `pyproject.toml`:

```toml
[tool.pydocfix]
# Rule selection (see "Rule selectors" for syntax details)
select = ["ALL"]
ignore = ["RIS"]

# Type annotation style: "signature" | "docstring" | "both" | omitted (default)
#   omitted    — PRM103/RTN103/YLD103 and PRM104/RTN104/YLD104 are all disabled
#   signature  — redundant docstring types flagged (x104); missing signature annotations flagged (x105)
#   docstring  — missing docstring types flagged (x103); redundant signature annotations flagged (x106)
#   both       — missing docstring types flagged (x103); missing signature annotations flagged (x105)
type_annotation_style = "signature"

# Preferred docstring style: "google" (default) | "numpy"
# Controls the format of auto-generated sections for plain (summary-only) docstrings.
# Existing Google/NumPy-style docstrings are always fixed in their detected style.
preferred_style = "google"

# Where __init__ arguments and raises should be documented.
# PRM001/RIS001 never fire on __init__; use CLS rules instead.
#   omitted  — CLS103–CLS106 and CLS203–CLS206 are disabled; __init__ is not checked (default)
#   "class"  — Args/Raises belong in the class docstring (CLS105/CLS106 enforce presence)
#   "init"   — Args/Raises belong in the __init__ docstring (CLS205/CLS206 enforce presence)
#   "both"   — Args/Raises may appear in either; presence/absence not enforced
class-docstring-style = "class"

# Paths/patterns to exclude (in addition to built-in defaults).
# Supports:
#   - Simple names: matched against each directory's name (e.g. "build", ".venv")
#   - Glob patterns: matched against paths relative to the project root
#     - "*"  matches any sequence of characters except "/"
#     - "**" matches zero or more path components
# Examples:
#   "tests/"                — exclude the top-level tests directory
#   "tests/**/fixtures/"    — exclude every fixtures/ directory under tests/
#   "src/generated_*.py"    — exclude files matching the glob
exclude = ["tests/", "docs/"]

# Skip section-level rules (PRM001, RTN001, YLD001, RIS001) for one-line docstrings (default: true)
skip_short_docstrings = true

# Treat Optional[T], T | None, and Union[T, None] as equivalent to T
# when comparing types in PRM101/RTN101/YLD101 (default: false)
allow_optional_shorthand = false

# Path to the baseline file (relative to pyproject.toml)
baseline = ".pydocfix-baseline.json"

# Output format: "full" (default, with source context) | "concise" (single-line)
output-format = "full"
```

## Rules

Each rule is classified as **safe** fix, **unsafe** fix, or report-only.

- **Safe** fixes can be applied automatically with `--fix` (no risk of changing semantics)
- **Unsafe** fixes require `--fix --unsafe-fixes` (may alter docstring meaning)

### Summary (SUM)

| Code | Default | Fix | Description |
|------|:-------:|:---:|-------------|
| SUM001 | ✅ | — | Missing summary line |
| SUM002 | ✅ | safe | Summary doesn't end with period |

### Parameters (PRM)

| Code | Default | Fix | Description |
|------|:-------:|:---:|-------------|
| PRM001 | ✅ | unsafe | Missing Args/Parameters section |
| PRM002 | ✅ | safe | Unnecessary Args/Parameters section |
| PRM003 | ✅ | safe | `self`/`cls` documented in docstring |
| PRM004 | ✅ | unsafe | Parameter in signature missing from docstring |
| PRM005 | ✅ | unsafe | Parameter in docstring not in signature |
| PRM006 | ✅ | unsafe | Parameter order mismatch |
| PRM007 | ✅ | unsafe | Duplicate parameter name |
| PRM008 | ✅ | — | Parameter has no description |
| PRM009 | ✅ | safe | Missing `*`/`**` prefix on `*args`/`**kwargs` |
| PRM101 | ✅ | unsafe | Docstring type doesn't match signature annotation |
| PRM102 | ✅ | unsafe | No type in docstring or signature |
| PRM103 | | unsafe | No type in docstring |
| PRM104 | | safe | Redundant type in docstring (signature has annotation) |
| PRM105 | | — | No type annotation in signature (`type_annotation_style = "signature"` or `"both"`) |
| PRM106 | | — | Redundant type annotation in signature (`type_annotation_style = "docstring"`) |
| PRM201 | ✅ | unsafe | Missing `optional` for parameter with default |
| PRM202 | | unsafe | Missing `default` for parameter with default |

### Returns (RTN)

| Code | Default | Fix | Description |
|------|:-------:|:---:|-------------|
| RTN001 | ✅ | unsafe | Missing Returns section |
| RTN002 | ✅ | safe | Unnecessary Returns section |
| RTN003 | ✅ | — | Returns entry has no description |
| RTN101 | ✅ | unsafe | Return type mismatch |
| RTN102 | ✅ | unsafe | No return type anywhere |
| RTN103 | | unsafe | No return type in docstring |
| RTN104 | | safe | Redundant return type in docstring |
| RTN105 | | — | No return type annotation in signature (`type_annotation_style = "signature"` or `"both"`) |
| RTN106 | | — | Redundant return type annotation in signature (`type_annotation_style = "docstring"`) |

### Yields (YLD)

| Code | Default | Fix | Description |
|------|:-------:|:---:|-------------|
| YLD001 | ✅ | unsafe | Missing Yields section |
| YLD002 | ✅ | safe | Unnecessary Yields section |
| YLD003 | ✅ | — | Yields entry has no description |
| YLD101 | ✅ | unsafe | Yield type mismatch |
| YLD102 | ✅ | unsafe | No yield type anywhere |
| YLD103 | | unsafe | No yield type in docstring |
| YLD104 | | safe | Redundant yield type in docstring |
| YLD105 | | — | No yield type annotation in signature (`type_annotation_style = "signature"` or `"both"`) |
| YLD106 | | — | Redundant yield type annotation in signature (`type_annotation_style = "docstring"`) |

### Raises (RIS)

| Code | Default | Fix | Description |
|------|:-------:|:---:|-------------|
| RIS001 | ✅ | unsafe | Missing Raises section |
| RIS002 | ✅ | safe | Unnecessary Raises section |
| RIS003 | ✅ | — | Raises entry has no description |
| RIS004 | ✅ | unsafe | Raised exception not documented |
| RIS005 | ✅ | unsafe | Documented exception not raised |

### Docstring (DOC)

| Code | Default | Fix | Description |
|------|:-------:|:---:|-------------|
| DOC001 | ✅ | unsafe | Section order doesn't match convention |
| DOC002 | ✅ | safe | Incorrect indentation of a docstring section entry |
| DOC003 | ✅ | safe | One-line docstring should be written on a single line |

### Class (CLS)

| Code | Default | Fix | Description |
|------|:-------:|:---:|-------------|
| CLS001 | ✅ | — | `__init__` has its own docstring but the class also has one |
| CLS101 | ✅ | safe | Class docstring has a Returns section |
| CLS102 | ✅ | safe | Class docstring has a Yields section |
| CLS103 | | unsafe | Class docstring has an Args section (`class_docstring_style = "init"`) |
| CLS104 | | unsafe | Class docstring has a Raises section (`class_docstring_style = "init"`) |
| CLS105 | | unsafe | Class docstring missing Args section (`class_docstring_style = "class"`) |
| CLS106 | | unsafe | Class docstring missing Raises section (`class_docstring_style = "class"`) |
| CLS201 | ✅ | safe | `__init__` docstring has a Returns section |
| CLS202 | ✅ | safe | `__init__` docstring has a Yields section |
| CLS203 | | unsafe | `__init__` docstring has an Args section (`class_docstring_style = "class"`) |
| CLS204 | | unsafe | `__init__` docstring has a Raises section (`class_docstring_style = "class"`) |
| CLS205 | | unsafe | `__init__` docstring missing Args section (`class_docstring_style = "init"`) |
| CLS206 | | unsafe | `__init__` docstring missing Raises section (`class_docstring_style = "init"`) |

> **Note:** PRM001 and RIS001 never fire on `__init__`; use CLS rules to enforce `__init__` docstring conventions.

## Rule selectors

`--select`, `--ignore` and their `pyproject.toml` equivalents accept the same **rule selector** syntax:

| Format | Example | Matches |
|--------|---------|----------|
| Exact code | `PRM001` | PRM001 only |
| Category prefix | `PRM` | All PRM rules |
| `ALL` | `ALL` | Every rule |

## Suppressing violations

### Inline suppression (`# noqa`)

Add a `# noqa` comment on the **closing `"""`** line to suppress violations for that docstring.

```python
def foo(x):
    """Short summary."""  # noqa                    # suppress all rules for this docstring

def bar(x):
    """Short summary."""  # noqa: PRM001            # suppress only PRM001

def baz(x):
    """Short summary."""  # noqa: PRM001, RTN001    # suppress multiple rules

# For multiline docstrings, put the comment on the closing """ line
def qux(x: int) -> int:
    """Short summary.

    Args:
        x: A value.
    """  # noqa: RTN001
    return x
```

Unused `# noqa` codes are reported as **NOQ001** (and removed by `--fix`).

### File-level suppression

Put a `# pydocfix: noqa` comment on its **own line** anywhere in the file to suppress violations for every docstring in the file.

```python
# pydocfix: noqa            # suppress all rules in this file
# pydocfix: noqa: PRM001    # suppress only PRM001 in this file
```

## Baseline

The baseline lets you record the current violation state of a project and suppress those existing violations on future runs — so only *new* violations are reported.
This makes gradual adoption easier: fix violations at your own pace.

```bash
# Record all current violations as the baseline
pydocfix check src/ --baseline .pydocfix-baseline.json --generate-baseline

# Future runs only report violations not in the baseline
pydocfix check src/ --baseline .pydocfix-baseline.json
```

Or configure the baseline path in `pyproject.toml` so you don't need the flag every time:

```toml
[tool.pydocfix]
baseline = ".pydocfix-baseline.json"
```

Then generate and use it:

```bash
pydocfix check src/ --generate-baseline    # write baseline
pydocfix check src/                        # only new violations reported
```

The baseline file is a JSON file that records violations by **symbol name** (e.g. `MyClass.my_method`) rather than line number, so it stays stable when unrelated code is added or removed.
Fixed violations are automatically removed from the baseline on the next run.

## pre-commit

Add to `.pre-commit-config.yaml`:

```yaml
repos:
  - repo: https://github.com/ryumasai/pydocfix
    rev: v0.1.0b2
    hooks:
      - id: pydocfix
```

To enable auto-fix:

```yaml
      - id: pydocfix
        args: [--fix]
```

## License

MIT
