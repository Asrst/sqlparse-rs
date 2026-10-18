# AGENTS.md

This file provides guidance to agents when working with code in this repository.

## Architecture constraints (non-obvious)

- **Two-crate workspace**: `sqlparse-core` (pure Rust, zero deps, the lexer + splitter) and `sqlparse-py` (PyO3 glue only). Any new functionality goes in `sqlparse-core`; `sqlparse-py` just re-exports it.
- **Token values are zero-copy slices of the input** — `tokenize()` returns `Vec<(&'static str, &str)>` where the second element is a slice of the caller's string. Do not allocate new strings in the lexer hot path.
- **`parse()` and `format()` are not Rust-backed yet** — they are delegated to the real `sqlparse` Python library. Implementing them in Rust (Tier 3) requires adding AST grouping types that mirror sqlparse's `sql.py` class hierarchy.
- **Keyword lookup uses binary search on a sorted static array** — not a hash map. `keywords.rs` must stay sorted. The generator (`tools/gen_keywords.py`) emits in sorted order.
- **Splitter uses a block stack, not a counter** — `BEGIN...END` blocks push/pop named entries. A bare `BEGIN;` pops the stack (it's a transaction begin, not a block). `CASE...END` does NOT push the stack. This is the most common source of parity bugs in the splitter.
- **Differential test design**: `differential_tokenize_parity` and `differential_split_parity` in `crates/sqlparse-core/tests/differential.rs` silently skip entire records where the Python oracle errored (non-UTF-8 input, etc.). They never fail on absent golden files — they skip. A test run that says "0 compared" is a missing corpus, not a pass.
- **Build: `lto = true, codegen-units = 1`** in the release profile (set in root `Cargo.toml`). Debug builds via `maturin develop` skip this; release builds are significantly slower to compile.
