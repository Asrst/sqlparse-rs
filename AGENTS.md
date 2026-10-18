# AGENTS.md

This file provides guidance to agents when working with code in this repository.

## Project

Rust + Python hybrid: a standalone engine for [python-sqlparse](https://github.com/andialbrecht/sqlparse) 0.6.0 workloads.
**Mixed maturin layout**: the `.so` (`sqlparse_rs`) + the `python/sqlparse_rs/` package are installed together.
**Independence by default**: `import sqlparse_rs` NEVER touches `sqlparse` (no import, no patch). The two engines coexist; `sqlparse` is used only by the benchmark/differential tooling as the oracle and baseline. Redirecting sqlparse's engine to Rust is an explicit user action: `sqlparse_rs.patch()` / `sqlparse_rs.unpatch()` (see `python/sqlparse_rs/_patch.py`).

## Commands

```bash
source "$HOME/.cargo/env"                                      # required every new shell — Rust not on PATH by default
source .venv/bin/activate                                      # activate Python venv
maturin develop                                                # build debug .so + install into venv (run from repo root)
maturin develop --release                                      # optimised build
cargo test -p sqlparse-core                                    # run all Rust tests (unit + differential)
cargo test -p sqlparse-core test_dollar_quoted -- --nocapture  # run a single Rust test by name filter
pytest tests/ -v                                               # all Python tests
pytest tests/test_compat.py::test_split_count -v               # single Python test
make check                                                     # corpus + goldens must exist first; runs cargo test -p sqlparse-core
make corpus goldens split-goldens                              # regenerate golden files (requires real sqlparse installed)
make gen-keywords                                              # regenerate crates/sqlparse-core/src/keywords.rs from installed sqlparse
```

## Critical non-obvious facts

- **`source "$HOME/.cargo/env"` every shell** — Rust toolchain is not on PATH by default in this devcontainer.
- **`maturin develop` must be run from the repo root** — the root `pyproject.toml` has `manifest-path = "crates/sqlparse-py/Cargo.toml"` and `python-source = "python"`.
- **Mixed layout**: `python/sqlparse_rs/__init__.py` + `python/sqlparse_rs/_patch.py` are the Python half; the `.so` lands at `python/sqlparse_rs/sqlparse_rs.cpython-*.so`. Import as `sqlparse_rs.sqlparse_rs` internally, or just `sqlparse_rs` (the package `__init__` re-exports everything).
- **Patching is explicit and opt-in** — `import sqlparse_rs` does NOT patch sqlparse. To serve `import sqlparse` from Rust, the user calls `sqlparse_rs.patch()` (reversible via `unpatch()`; `SQLPARSE_RS_NO_PATCH=1` is a kill-switch). In tests this is done by the autouse fixture in `test_compat.py`. The independence contract is pinned by `tests/test_independence.py` — keep both green.
- **`keywords.rs` is GENERATED** — do not hand-edit it. Run `make gen-keywords` (requires the real `sqlparse` installed in the venv) to regenerate from the live keyword dicts.
- **Golden files are the compat referee**. When sqlparse behaviour is ambiguous, run `python tools/oracle.py tokenize` or `python tools/oracle.py split` against real sqlparse — never guess. `corpus/inputs.bin` is NOT committed; regenerate with `make corpus`.
- **Differential tests skip, not fail, when goldens are absent** — `cargo test` stays green before golden setup.
- **FIRST match wins, never longest match** — the 50-rule scanner in `lexer.rs` mirrors `keywords.SQL_REGEX` order exactly. `PORT_MATRIX.md` is the ground truth. Do not reorder rules.
- **CASE...END does NOT protect `;`** — only `BEGIN...END` blocks do. Verified sqlparse quirk.
- **`sqlparse-core` has zero dependencies** — no `regex` crate, no `unsafe`. Keep it that way. `pyo3` lives only in `sqlparse-py`.

## Architecture

```
pyproject.toml             — maturin config: module-name=sqlparse_rs, python-source=python
python/sqlparse_rs/
  __init__.py              — re-exports tokenize/split from .so; calls _patch.install()
  _patch.py                — idempotent monkey-patch of sqlparse.lexer.Lexer.get_tokens
crates/sqlparse-core/src/
  ttype.rs      — &'static str token type constants (exact Python str(ttype) strings)
  keywords.rs   — GENERATED 803-entry keyword map + binary search; first-dict-wins priority
  lexer.rs      — 50-rule character scanner; zero-copy slices; O(n), no backtracking
  spans.rs      — find_delimited_spans prepass ($-quotes, /* */ hint detection)
  splitter.rs   — statement_splitter.py port; state machine with block stack
  lib.rs        — pub use lexer::tokenize, splitter::split
crates/sqlparse-py/src/lib.rs  — PyO3 glue; exposes tokenize()+split() as sqlparse_rs.sqlparse_rs
corpus/
  sql/*.sql              — 24 fixture files (committed)
  goldens.txt            — committed tokenize goldens (real sqlparse 0.6.0 output)
  goldens-split.txt      — committed split goldens
  inputs.bin             — NOT committed; regenerate with `make corpus`
tools/
  oracle.py              — differential oracle (tokenize|split|words modes, hex output)
  gen_keywords.py        — regenerates keywords.rs
  build_corpus.py        — builds corpus/inputs.bin
  dump_regex_table.py    — generates PORT_MATRIX.md
tests/
  test_wrapper.py        — Rust parity tests (sqlparse_rs vs real sqlparse, byte-identical)
  test_compat.py         — Python API compat tests (45 tests: split/parse/format/tokenize/patch)
```
