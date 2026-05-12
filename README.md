# pydocsync

`pydocsync` is a fast Python signature-docstring synchronizer.

It checks only structural drift between code and docstrings: arguments, return sections, yield sections, and raised exceptions. It is intentionally not a general docstring linter. It does not enforce prose style, summary punctuation, type annotation policy, class docstring policy, inline suppression, or baselines.

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

`--fix` applies safe fixes by default. `--unsafe-fixes` also allows generated section stubs and other edits that may need human review.

## Configuration

Configuration is optional. When present, `pydocsync` reads `[tool.pydocsync]` from the nearest `pyproject.toml`.

```toml
[tool.pydocsync]
ignore = ["raises"]
exclude = ["build/**", "tests/fixtures/**"]
```

Only `ignore` and `exclude` are supported.

## Rules

Rule IDs are readable and stable:

| Rule | Fix | Description |
| --- | --- | --- |
| `args-section-missing` | unsafe | Signature has documentable arguments but the docstring has no Args/Parameters section |
| `args-section-extra` | safe | Docstring has an Args/Parameters section but the signature has no documentable arguments |
| `args-receiver-documented` | safe | Docstring documents `self` or `cls` |
| `args-param-missing` | unsafe | Signature argument is missing from the docstring |
| `args-param-extra` | unsafe | Docstring argument is not in the signature |
| `args-param-out-of-order` | unsafe | Docstring argument order differs from the signature |
| `args-param-duplicate` | unsafe | Docstring documents the same argument more than once |
| `args-vararg-marker-missing` | safe | Docstring omits `*` or `**` for `*args` or `**kwargs` |
| `returns-section-missing` | unsafe | Function returns a value but the docstring has no Returns section |
| `returns-section-extra` | safe | Docstring has a Returns section but the function returns no value |
| `yields-section-missing` | unsafe | Generator yields values but the docstring has no Yields section |
| `yields-section-extra` | safe | Docstring has a Yields section but the function is not a generator |
| `raises-section-missing` | unsafe | Function raises exceptions but the docstring has no Raises section |
| `raises-section-extra` | safe | Docstring has a Raises section but the function raises no exceptions |
| `raises-exception-missing` | unsafe | Raised exception is missing from the Raises section |
| `raises-exception-extra` | unsafe | Raises entry documents an exception not raised by the function |

`--ignore` and `ignore` accept exact rule IDs or group prefixes. For example, `raises` ignores every `raises-*` rule, and `args-section` ignores both argument section rules.

Missing-section rules are intentionally lenient: they fire only when the docstring already uses another structured section. A short summary-only docstring is left alone.

## Scope

`pydocsync` is designed to stay small enough to run often and simple enough to upstream or replace. Anything beyond signature-docstring synchronization belongs in a formatter, a type checker, or a broader linter.
