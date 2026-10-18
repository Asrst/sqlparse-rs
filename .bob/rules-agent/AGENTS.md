# AGENTS.md

This file provides guidance to agents when working with code in this repository.

## Coding rules (non-obvious)

- **Zero-dependency constraint on `sqlparse-core`**: the crate has no entries in `[dependencies]`. Do not add any. The `regex` crate must never enter the token path. `pyo3` belongs only in `sqlparse-py`.
- **`#![forbid(unsafe_code)]`** is declared on `sqlparse-core`. Any unsafe block will fail to compile.
- **Token type strings must match Python exactly** — use constants from `crates/sqlparse-core/src/ttype.rs`. Do not inline string literals like `"Token.Keyword"` elsewhere; the differential tests compare these strings to real sqlparse output byte-for-byte.
- **`keywords.rs` must not be hand-edited** — run `make gen-keywords` instead. The file is generated from the live sqlparse install; hand edits will be overwritten and will also drift from the real keyword dicts.
- **Rule order in `lexer.rs` is the spec** — FIRST match wins. Any reorder changes output. Before modifying rule order, consult `PORT_MATRIX.md` (regenerate with `make port-matrix` if stale).
- **`maturin develop` from repo root only** — running it from inside `crates/sqlparse-py/` installs under a different name and breaks `import sqlparse_rs`.
- **`corpus/inputs.bin` is not committed** — tests that read it will skip (not fail) when it's absent. Regenerate: `make corpus`.
- Python tests in `tests/test_compat.py` that call `rs.parse()` / `rs.format()` delegate to the real `sqlparse` library — they pass as long as real sqlparse is installed in the venv. They do NOT test Rust code.
