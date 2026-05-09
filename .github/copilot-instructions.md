# Development Guidelines

## Tooling

- Use `uv` as the Python package manager (not pip, poetry, or pipenv)
- The implementation is Rust-first. Run tests with `cargo test --workspace`.
- Run formatting with `cargo fmt --all`.
- Run pydocfix itself with `uv run pydocfix check <path>`
- Build the Python wheel with `uv run maturin build --release --out dist`.

## Commits

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
feat:     new rule or user-visible feature
fix:      bug fix
refactor: internal restructuring without behaviour change
test:     adding or updating tests
docs:     documentation only
chore:    maintenance (deps, CI, tooling)
```

## Project Structure

```
crates/pydocfix-scanner/  # Python source scanner and host extraction
crates/pydocfix-core/     # Analysis model, built-in rules, diagnostics, fixes
crates/pydocfix-cli/      # CLI args, config, file discovery, rendering, baseline, fixing, noqa
pyproject.toml            # maturin binary wheel configuration
```

## Key Conventions

- Keep CLI orchestration thin; put focused behavior in modules under `crates/pydocfix-cli/src/`.
- Rule diagnostics and fixes belong in `crates/pydocfix-core/src/lib.rs` until a rule-module split is introduced.
- Preserve file-absolute UTF-8 byte offsets internally; render line/column only at the CLI boundary.
- Prefer root-cause parser or semantic fixes in `docstring-cst` over pydocfix-side workarounds.
- Plugin compatibility was intentionally removed in the Rust migration.
