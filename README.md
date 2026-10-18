
# sqlparse-rs

A Rust port of [python-sqlparse](https://github.com/andialbrecht/sqlparse) 0.6.0 exposed to Python
via [PyO3](https://github.com/PyO3/pyo3) — designed as a **drop-in replacement**.

## Status

| Tier | Component | Status |
|------|-----------|--------|
| 1 | Lexer / tokenizer | ✅ Complete — 6.8× faster, byte-identical to sqlparse 0.6.0 |
| 1 | Statement splitter (`split()`) | ✅ Complete — BEGIN/END blocks, GO, dollar-quotes, issues 762/809/826 |
| 2 | PyO3 bindings (`parse()`, `tokenize()`) | ✅ Working MVP |
| 3 | AST grouping (`parse()` full tree) | 🚧 In progress |
| 3 | Formatter (`format()`) | 🚧 In progress |

**Drop-in target:** `import sqlparse` works unchanged — same `parse()`, `format()`, `split()`,
`parsestream()`, `tokenize()` signatures, same token type hierarchy, same `Statement`/`Identifier`
AST node classes.

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
# Existing code — no changes needed
import sqlparse

# split
sqlparse.split("SELECT 1; SELECT 2")
# → ['SELECT 1', 'SELECT 2']

# parse
stmts = sqlparse.parse("SELECT a, b FROM users WHERE id = 1")
stmt = stmts[0]
print(stmt.get_type())  # 'SELECT'
for tok in stmt.flatten():
    print(tok.ttype, repr(tok.value))

# format
sqlparse.format("select a,b from t", keyword_case="upper", reindent=True)
```

## Repo layout

```
Cargo.toml                pure Rust crate config (cdylib)
pyproject.toml            maturin build config; module-name = "sqlparse._rust"
python/sqlparse/          Python shim package (the drop-in layer)
  __init__.py             re-exports parse/format/split/parsestream
  sql.py                  re-exports PyO3 AST node classes
  tokens.py               token type hierarchy (delegates to sqlparse.tokens)
  exceptions.py           SQLParseError
src/
  lib.rs                  PyO3 module entry point
  keywords.rs             803-entry keyword table (GENERATED — see tools/)
  lexer.rs                50-rule scanner, SQL_REGEX order 1:1 with sqlparse
  splitter.rs             statement_splitter.py port
  grouping.rs             grouping passes (WIP)
  formatter.rs            format() options (WIP)
  sql_types.rs            PyO3 Token/TokenList/Statement/Identifier classes
tests/
  test_compat.py          compatibility tests vs sqlparse
corpus/sql/               micro-cases per lexer trap + realistic shapes
corpus/goldens*.txt       committed golden outputs from real sqlparse 0.6.0
tools/oracle.py           differential oracle (tokenize | split | words)
tools/gen_keywords.py     keywords.rs generator (first-dict-wins priority)
PORT_MATRIX.md            generated spec of the 50 lexer rules
.bob/skills/
  pyo3-maturin/           PyO3 0.22 + maturin reference skill
  sqlparse-compat/        sqlparse public API compatibility reference skill
.bob/mcp.json             MCP servers: crates-io, fetcher (docs.rs)
```

## Development commands

```bash
# Regenerate keyword table from installed sqlparse
python tools/gen_keywords.py > src/keywords.rs

# Regenerate golden files (referee outputs from real sqlparse)
python tools/oracle.py tokenize < corpus/inputs.bin > corpus/goldens.txt
python tools/oracle.py split    < corpus/inputs.bin > corpus/goldens_split.txt

# Rust unit + differential tests
cargo test

# Build debug .so and run Python compat tests
maturin develop && pytest tests/ -v

# Build optimised .so
maturin develop --release

# Benchmark
cargo run --release -p sqlparse-core --bin bench < corpus/inputs.bin

# Run a single Python test
pytest tests/test_compat.py::test_split_begin_end_not_split -v

# Run a single Rust test
cargo test test_dollar_quoted -- --nocapture
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

- Unicode: Python `re` `\w`/`\d` semantics approximated (`is_alphanumeric`/`is_numeric`);
  combining marks may differ on exotic NFD input. ASCII SQL is unaffected.
- `Lexer.add_keywords()` runtime customisation not exposed — regenerate `keywords.rs` instead.
- `format()` reindent and grouping are work-in-progress; fall back to Python sqlparse if needed.

## License

BSD-3-Clause; see LICENSE. Derivative work of
[python-sqlparse](https://github.com/andialbrecht/sqlparse) (BSD-3-Clause).
