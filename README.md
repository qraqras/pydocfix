# pydocsync

`pydocsync` is a fast Python signature-docstring synchronizer.

It checks only high-confidence structural drift between code and docstrings: documented argument entries against function signatures and documented return entries against clear value-returning behavior. It is intentionally not a general docstring linter. It does not enforce prose style, summary punctuation, type annotation policy, class docstring policy, return sections, yield sections, raises sections, inline suppression, or baselines.

## Install

```bash
pip install pydocsync
```

For local development:

```bash
cargo run --bin pydocsync -- path/to/package
```

## Usage

```bash
pydocsync src tests
pydocsync --fix src
pydocsync --diff src
pydocsync --fix --unsafe-fixes src
```

The CLI accepts paths directly. There is no subcommand.

Options:

```text
--fix
--diff
--unsafe-fixes
--ignore <RULE[,RULE...]>
--exclude <PATTERN[,PATTERN...]>
--jobs <N>
```

`--fix` applies safe fixes by default. `--unsafe-fixes` also allows generated entry edits that may need human review.

## Configuration

Configuration is optional. When present, `pydocsync` reads `[tool.pydocsync]` from the nearest `pyproject.toml`.

```toml
[tool.pydocsync]
ignore = ["args-param-extra"]
exclude = ["build/**", "tests/fixtures/**"]
```

Only `ignore` and `exclude` are supported.

## Rules

Rule IDs are readable and stable:

| Rule | Fix | Description |
| --- | --- | --- |
| `args-receiver-documented` | safe | Docstring documents `self` or `cls` |
| `args-param-missing` | unsafe | Required signature argument is missing from an existing Args/Parameters section |
| `args-param-extra` | unsafe | Docstring argument is not in a non-variadic function signature |
| `args-param-duplicate` | unsafe | Docstring documents the same argument more than once |
| `returns-entry-missing` | none | Existing Returns section has no return entry for a clearly value-returning function |
| `returns-entry-extra` | none | Return entry is not matched by a value return or meaningful return annotation |

`--ignore` and `ignore` accept exact rule IDs or group prefixes. For example, `args` ignores every `args-*` rule and `returns` ignores every `returns-*` rule.

Rules intentionally prefer false negatives over false positives. pydocsync does not require or remove Args/Parameters or Returns sections, does not enforce parameter order, does not require `*args`/`**kwargs` spelling in docstrings, and treats variadic signatures conservatively. Return entries documenting `None` are treated as intentional and are not reported as extra.

Return, Yield, and Raises sections are intentionally ignored at the section level. Generator behavior and exception behavior are often part of an API contract rather than something pydocsync can infer reliably from local syntax.

## Scope

`pydocsync` is designed to stay small enough to run often and simple enough to upstream or replace. Anything beyond signature-docstring synchronization belongs in a formatter, a type checker, or a broader linter.
