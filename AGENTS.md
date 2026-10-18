# AGENTS.md

This file provides guidance to agents when working with code in this repository.

## Project

Rust + Python hybrid package named **sqlparse-rs** — a drop-in replacement for
[python-sqlparse](https://github.com/andialbrecht/sqlparse) 0.6.0. Rust core (PyO3 bindings via
maturin), Python shim at `python/sqlparse/`. The `.so` is installed as `sqlparse/_rust`.

## Commands

```bash
source "$HOME/.cargo/env"            # activate Rust (required every shell session in this devcontainer)
maturin develop                      # build debug .so, install into venv
maturin develop --release            # optimised build
cargo test                           # Rust unit + differential tests
pytest tests/ -v                     # Python compat tests
pytest tests/test_compat.py::test_split_count -v   # single test
cargo test test_dollar_quoted -- --nocapture        # single Rust test
```

## Critical non-obvious facts

- **`source "$HOME/.cargo/env"` is required** — Rust is not on PATH by default in this devcontainer. Every shell session needs it.
- **Module name is `sqlparse._rust`** (set in `pyproject.toml`). The Python package lives in `python/sqlparse/`, not root. `maturin develop` installs it *as* `sqlparse` — so `import sqlparse` picks up the Rust-backed version.
- **`keywords.rs` is GENERATED** from the real sqlparse keyword dicts via `tools/gen_keywords.py`. Do not hand-edit it; run the generator instead.
- **Golden files at `corpus/goldens*.txt`** are the compat referee. When sqlparse behaviour is ambiguous, run `tools/oracle.py` against real sqlparse — never guess.
- **`CASE...END` does NOT protect `;`** in the splitter — only `BEGIN...END` blocks do. This is a verified sqlparse quirk, not a bug.
- **Tier 3 (grouping, format) is in-progress** — `grouping.rs` and `formatter.rs` exist but are incomplete. `split()` and `tokenize()` are complete and byte-identical.

## Architecture

```
src/keywords.rs   ← generated 803-entry keyword map + 50 regex rules
src/lexer.rs      ← character-by-character scanner using the rule table
src/splitter.rs   ← statement splitter (port of statement_splitter.py)
src/grouping.rs   ← AST grouping passes (WIP)
src/formatter.rs  ← format() options (WIP)
src/sql_types.rs  ← PyO3 Token/TokenList/Statement/Identifier pyclass structs
src/lib.rs        ← pymodule entry, exposes parse/format/split/tokenize
python/sqlparse/  ← Python shim: __init__.py sql.py tokens.py exceptions.py
```

## Skills available (activate with use_skill)

- `pyo3-maturin` — PyO3 0.22 API patterns, maturin commands, type mapping, common errors
- `sqlparse-compat` — exact sqlparse public API contract, token hierarchy, edge cases, golden-file testing

## MCPs configured (`.bob/mcp.json`)

- `crates-io` — search crates.io for Rust crate versions/features
- `fetcher` — fetch any URL as clean text (use for docs.rs pages: `https://docs.rs/pyo3/latest/pyo3/`)
