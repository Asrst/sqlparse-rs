
# sqlparse-rs

A Rust port of [python-sqlparse](https://github.com/andialbrecht/sqlparse) 0.6.0 exposed to Python
via [PyO3](https://github.com/PyO3/pyo3) — a **drop-in replacement**.

## Status

| Tier | Component | Status |
|------|-----------|--------|
| 1 | Lexer / tokenizer | ✅ Complete — 6.8× faster, byte-identical to sqlparse 0.6.0 |
| 1 | Statement splitter (`split()`) | ✅ Complete — BEGIN/END blocks, GO, dollar-quotes, issues 762/809/826 |
| 2 | Drop-in `import sqlparse` shim | ✅ Complete — Rust lexer patched into sqlparse's engine |
| 2 | `parse()` / `format()` / `parsestream()` | ✅ Complete — Rust tokens; `parse()`/`format()` use Rust grouping (Tier 3) |
| 3 | Full Rust grouping + formatter | ✅ Complete — Rust AST grouping + formatter; `parse()`/`format()` no longer touch Python's grouping engine |

**Drop-in today:** `import sqlparse_rs` once (or at startup), then `import sqlparse` works
unchanged — same `parse()`, `format()`, `split()`, `parsestream()`, `tokenize()` signatures.
The Rust lexer is automatically wired into sqlparse's grouping and formatting engine.

## Benchmark (Tier 1 — lexer + splitter)

1.35 MB corpus, equal work asserted (103,800 tokens both sides), Rust release build:

| implementation        | time    | throughput |
|-----------------------|---------|------------|
| python-sqlparse 0.6.0 | 0.777 s | 1.6 MB/s   |
| sqlparse-rs           | 0.114 s | 11.3 MB/s  |
| **speedup**           | **~6.8×** |          |

> Honest note: this is the correctness-first scanner. The ASCII fast-path / byte-level dispatch
> version has obvious headroom; 10–50× is aspirational — quote the measured table above.

## Quickstart

```bash
# Rust toolchain (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

# Python deps
pip install maturin pytest sqlparse==0.6.0

# Build and install into current venv
maturin develop --release

# Run all tests
pytest tests/
cargo test
```

Requires Rust 1.75+ (tested with 1.99.1), Python 3.8+.

## Usage (drop-in)

```python
# One-time import at app startup (or in your conftest.py / __init__.py)
import sqlparse_rs  # patches sqlparse's lexer automatically

# Then use sqlparse exactly as before — no other changes needed
import sqlparse

# split — uses Rust state-machine splitter
sqlparse.split("SELECT 1; SELECT 2")
# → ['SELECT 1', 'SELECT 2']

# parse — Python grouping/AST over Rust tokens
stmts = sqlparse.parse("SELECT a, b FROM users WHERE id = 1")
stmt = stmts[0]
print(stmt.get_type())  # 'SELECT'
for tok in stmt.flatten():
    print(tok.ttype, repr(tok.value))

# format — Python formatter over Rust tokens
sqlparse.format("select a,b from t", keyword_case="upper", reindent=True)

# low-level tokenize — direct Rust access
import sqlparse_rs
list(sqlparse_rs.tokenize("SELECT 1"))
# → [('Token.Keyword.DML', 'SELECT'), ('Token.Text.Whitespace', ' '), ...]
```

## Repo layout

```
pyproject.toml                maturin build config (mixed Rust/Python layout)
python/sqlparse_rs/
  __init__.py               re-exports tokenize/split, installs Rust lexer patch
  _patch.py                 monkey-patches sqlparse.lexer.Lexer.get_tokens
crates/sqlparse-core/src/
  ttype.rs                  &'static str token type constants
  keywords.rs               GENERATED 803-entry keyword map + binary search
  lexer.rs                  50-rule character scanner; zero-copy slices; O(n)
  spans.rs                  find_delimited_spans prepass ($-quotes, /* */ hints)
  splitter.rs               statement_splitter.py port; state machine
  lib.rs                    pub use lexer::tokenize, splitter::split
crates/sqlparse-py/src/lib.rs   PyO3 glue; exposes tokenize()+split() as sqlparse_rs
tests/
  test_compat.py            compatibility tests vs sqlparse (45 tests)
  test_wrapper.py           Rust parity vs real sqlparse, byte-identical
corpus/sql/                 24 fixture files (committed)
corpus/goldens.txt          committed tokenize goldens (real sqlparse 0.6.0)
corpus/goldens-split.txt    committed split goldens
tools/oracle.py             differential oracle (tokenize|split|words modes)
tools/gen_keywords.py       regenerates keywords.rs
PORT_MATRIX.md              generated spec of the 50 lexer rules
docs/DECISIONS.md           architecture decisions
```

## Development commands

```bash
# Build debug .so + install Python package into venv (run from repo root)
maturin develop

# Build optimised release .so
maturin develop --release

# All Python tests
pytest tests/ -v

# Single Python test
pytest tests/test_compat.py::test_split_begin_end_not_split -v

# All Rust tests (unit + differential parity)
cargo test -p sqlparse-core

# Single Rust test
cargo test -p sqlparse-core test_dollar_quoted -- --nocapture

# Regenerate keyword table from installed sqlparse
make gen-keywords

# Regenerate golden files (requires real sqlparse in venv)
make corpus goldens split-goldens

# Run parity gate
make check

# Benchmark
cargo run --release -p sqlparse-core --bin bench < corpus/inputs.bin
```

## What was hard (and is now covered by tests)

- Comments CONSUME their trailing newline (`m.group()` includes the terminator); `#` is a
  comment only when followed by a space; `--+`/`# +` are the hint variants.
- Number rules backtrack: `1.5x` → `Float "1."` + `Name "5x"`; `12abc` → `Integer "1"` + `Name "2abc"`.
- Unclosed strings/quotes do NOT swallow to EOF — the quote becomes `Error`, rest lexes normally.
- Word continuation includes `$` and `#`: `a#>>b` is `Name "a#"` + `>>`.
- Rule 13 (`CASE|IN|VALUES|USING|FROM|AS`) fires BEFORE the word rule — `FROM` is plain `Keyword`.
- Rule 16 (lookbehind) IS alive: `t.select` → the `select` part is `Token.Name`.
- `find_delimited_spans` collects openers and closers in independent passes.
- `split()` strips every statement; `CASE` does NOT protect `;` — only `BEGIN...END` blocks do.
- `BEGIN TRANSACTION`/`BEGIN WORK` must NOT open a block (issue826).

## Honest limitations

- **`import sqlparse_rs` must precede the first `import sqlparse`** for the Rust lexer to be
  active. In practice: add it to your `conftest.py`, app `__init__.py`, or entry-point.
- Unicode: Python `re` `\w`/`\d` semantics approximated (`is_alphanumeric`/`is_numeric`);
  combining marks may differ on exotic NFD input. ASCII SQL is unaffected.
- `Lexer.add_keywords()` runtime customisation not exposed — regenerate `keywords.rs` instead.
- `parsestream()` still uses Python's `FilterStack`/`StatementSplitter` internally; only `parse()` and `format()` are fully Rust.
- `reindent_aligned` and `right_margin` format options fall back to Python sqlparse (rare options).

## License

BSD-3-Clause; see LICENSE. Derivative work of
[python-sqlparse](https://github.com/andialbrecht/sqlparse) (BSD-3-Clause).
