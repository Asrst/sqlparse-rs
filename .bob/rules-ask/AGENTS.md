# AGENTS.md

This file provides guidance to agents when working with code in this repository.

## Documentation context (non-obvious)

- The repository is named `sqlparser-rs` but the actual crate/extension is `sqlparse-core` / `sqlparse_rs` — the repo name is a devcontainer artifact and does not match any package or import name.
- `PORT_MATRIX.md` is the ground-truth specification for all 50 lexer rules. Regenerate it with `make port-matrix` when investigating scanner behaviour. It is authoritative over any written description.
- `docs/PLAN.md` contains the original design rationale and trap list for the scanner — useful context when debugging parity failures, even though the implementation is now complete.
- `corpus/goldens.txt` and `corpus/goldens-split.txt` are committed and are the single source of truth for correct tokenize/split output. They were generated from real sqlparse 0.6.0, not from this codebase.
- `corpus/inputs.bin` is NOT committed (it's binary). It is built from `corpus/sql/*.sql` via `make corpus`.
- `tests/test_compat.py` imports `sqlparse_rs` for tokenize/split tests but imports the real `sqlparse` for parse/format tests. The two are different modules — the Rust extension does not (yet) provide `parse()` or `format()`.
