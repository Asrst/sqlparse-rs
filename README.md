# sqlparse-rs

A Rust port of [python-sqlparse](https://github.com/andialbrecht/sqlparse) 0.6.0 exposed to Python
via [PyO3](https://github.com/PyO3/pyo3) — a **standalone engine with the same API**. It has ZERO
runtime dependency on `sqlparse`: the two packages install side by side, work independently, and
`import sqlparse_rs` never touches `sqlparse`. Redirecting `sqlparse` itself to the Rust engine is
an explicit, reversible opt-in: `sqlparse_rs.patch()` … `sqlparse_rs.unpatch()`.

## Status

| Tier | Component | Status |
|------|-----------|--------|
| 1 | Lexer / tokenizer | ✅ Complete — 6.8× faster, byte-identical to sqlparse 0.6.0 |
| 1 | Statement splitter (`split()`) | ✅ Complete — BEGIN/END blocks, GO, dollar-quotes |
| 2 | Opt-in `patch()` / `unpatch()` compat layer | ✅ Complete — explicit, reversible; never automatic |
| 2 | `parse()` / `format()` / `parsestream()` | ✅ Complete — Rust tokens, grouping, and formatter |
| 3 | Full Rust grouping + formatter | ✅ Complete — Rust AST grouping + formatter; no Python grouping engine |
| 3 | Own `sql` / `tokens` / `parsestream` | ✅ Complete — zero `sqlparse` runtime dependency |

### Evidence — four banked proofs

- **Proof 1 — correctness**: **2,040 records / 93,624 tokens, zero diffs** vs
  real `sqlparse==0.6.0` for `tokenize` AND `split` — 40 corpus records
  (traps, 9-dialect keyword sweep, issue601/issue39, 10k IN list, 10k
  unclosed `$$`) + 2,000 fuzz statements (fuzz gate caught and fixed one real
  bug: `%S` placeholder vs IGNORECASE). Gates: `cargo test` goldens +
  live-vs-live `tests/test_parity.py` → `tests/parity-report.md`.
- **Proof 2 — speed** (equal-work validated, sha256 stream checksums,
  min-of-3): dump tokenize **5.7x** (8.8 MB / 2.79M tokens), dump split
  **8.7x** (7,600 stmts), query-log fingerprint **4.9x** (100k queries).
  End-to-end through the PyO3 tuple boundary; the pure-Rust scanner does
  11.3 MB/s (~6.8x on the corpus bench). Details: `benchmarks/RESULTS.md`.
- **Proof 3 — safety**: unclosed `$$` at 10 MB — Python 0.6.0 **DNFs
  (>120 s)**, Rust finishes **linear** in 18.6 s; comment-only inputs linear
  for both (~3x gap). No backtracking engine left in the hot path.
  Visual: `benchmarks/scaling.png`.
- **Proof 4 — downstream**: the upstream test suite (test_tokenize +
  test_split, 120 tests) run against the explicit `sqlparse`→`sqlparse_rs`
  alias shim (opt-in, `-p downstream_shim`) with
  the FULL surface served by Rust: **120 passed / 0 failed** — tokenize,
  split (incl. `strip_semicolon`), parse, isinstance checks, and
  `str(statement)` round-trips, byte-identical. Unmodified downstream script
  (`examples/statement_report.py`) byte-identical across engines.
  Report: `tests/downstream-report.md`.

## Benchmark

1.35 MB corpus, equal work asserted (103,800 tokens both sides), Rust release build:

| implementation        | time    | throughput |
|-----------------------|---------|------------|
| python-sqlparse 0.6.0 | 0.777 s | 1.6 MB/s   |
| sqlparse-rs           | 0.114 s | 11.3 MB/s  |
| **speedup**           | **~6.8×** |          |

## Installation

```bash
# Rust toolchain (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

pip install maturin
maturin develop --release
```

Requires Rust 1.75+, Python 3.8+.

## Quick usage

`sqlparse_rs` is a **complete replacement** — one import, no changes to the rest of your code.

```python
import sqlparse_rs as sqlparse
```

That's it. Every `sqlparse` function and sub-module is available on `sqlparse_rs`.

---

### `split()` — split SQL into individual statements

```python
import sqlparse_rs as sqlparse

sqlparse.split("SELECT 1; SELECT 2; SELECT 3")
# → ['SELECT 1;', 'SELECT 2;', 'SELECT 3']
```

Handles dollar-quoted strings — inner `;` are never treated as statement boundaries:

```python
sqlparse.split("SELECT $$hello; world$$ AS x; SELECT 2")
# → ['SELECT $$hello; world$$ AS x;', 'SELECT 2']
```

Handles `BEGIN...END` blocks — inner `;` are preserved:

```python
sql = """
CREATE FUNCTION foo() RETURNS void AS $$
BEGIN
    UPDATE t SET x = 1;
    UPDATE t SET y = 2;
END;
$$ LANGUAGE plpgsql;
SELECT 1;
"""
sqlparse.split(sql)
# → 2 statements: the CREATE FUNCTION and SELECT 1
```

---

### `parse()` — parse into an AST

```python
import sqlparse_rs as sqlparse

stmts = sqlparse.parse("SELECT id, name FROM users WHERE id = 1")
stmt  = stmts[0]

stmt.get_type()   # → 'SELECT'
str(stmt)         # → 'SELECT id, name FROM users WHERE id = 1'
```

Works for all DML types:

```python
sqlparse.parse("SELECT a FROM b")[0].get_type()          # → 'SELECT'
sqlparse.parse("INSERT INTO t VALUES (1)")[0].get_type() # → 'INSERT'
sqlparse.parse("UPDATE t SET x=1")[0].get_type()         # → 'UPDATE'
sqlparse.parse("DELETE FROM t WHERE id=1")[0].get_type() # → 'DELETE'
```

Flatten a statement to individual tokens:

```python
for tok in sqlparse.parse("SELECT id, name FROM users WHERE id = 1")[0].flatten():
    print(tok.ttype, repr(tok.value))

# Token.Keyword.DML   'SELECT'
# Token.Text.Whitespace ' '
# Token.Name          'id'
# Token.Punctuation   ','
# Token.Text.Whitespace ' '
# Token.Name          'name'
# Token.Text.Whitespace ' '
# Token.Keyword       'FROM'
# Token.Text.Whitespace ' '
# Token.Name          'users'
# Token.Text.Whitespace ' '
# Token.Keyword       'WHERE'
# Token.Text.Whitespace ' '
# Token.Name          'id'
# Token.Text.Whitespace ' '
# Token.Operator.Comparison '='
# Token.Text.Whitespace ' '
# Token.Literal.Number.Integer '1'
```

Access typed AST nodes via `sqlparse_rs.sql`:

```python
import sqlparse_rs.sql as sql

stmt = sqlparse_rs.parse("SELECT a FROM t WHERE x = 1 AND y = 2")[0]

# WHERE clause is grouped into a Where node
where = [tok for tok in stmt.tokens if isinstance(tok, sql.Where)]
str(where[0])   # → 'WHERE x = 1 AND y = 2'

# Identifier alias
stmt = sqlparse_rs.parse("SELECT foo AS bar FROM users")[0]
for id_ in stmt.get_sublists():
    print(id_.get_name(), id_.get_alias())
# bar  bar
# users  None
```

CTEs and subqueries work as a single statement:

```python
sqlparse_rs.parse("WITH cte AS (SELECT id FROM users) SELECT * FROM cte WHERE id > 5")[0].get_type()
# → 'SELECT'

sqlparse_rs.parse("SELECT * FROM (SELECT a FROM b) sub")[0].get_type()
# → 'SELECT'
```

---

### `format()` — format SQL

```python
import sqlparse_rs as sqlparse

# Reindent + uppercase keywords
sqlparse.format(
    "select id, name from users where id = 1 order by id",
    reindent=True,
    keyword_case="upper",
)
# → 'SELECT id, name\nFROM users\nWHERE id = 1\nORDER BY id'

# Strip extra whitespace
sqlparse.format("SELECT   a    FROM   b", strip_whitespace=True)
# → 'SELECT a FROM b'

# Strip comments
sqlparse.format("SELECT a -- comment\nFROM b /* block */", strip_comments=True, strip_whitespace=True)
# → 'SELECT a FROM b'

# Lowercase keywords
sqlparse.format("SELECT A FROM B WHERE X = 1", keyword_case="lower")
# → 'select A from B where X = 1'

# Lowercase identifiers
sqlparse.format("SELECT MyCol FROM MyTable", identifier_case="lower")
# → 'SELECT mycol FROM mytable'
```

---

### `tokenize()` — raw token pairs

Returns a list of `(token_type_string, value)` tuples — zero-copy, O(n), byte-identical to sqlparse:

```python
import sqlparse_rs as sqlparse

sqlparse.tokenize("SELECT id, name FROM users WHERE id = 1")
# → [
#     ('Token.Keyword.DML',            'SELECT'),
#     ('Token.Text.Whitespace',        ' '),
#     ('Token.Name',                   'id'),
#     ('Token.Punctuation',            ','),
#     ('Token.Text.Whitespace',        ' '),
#     ('Token.Name',                   'name'),
#     ('Token.Text.Whitespace',        ' '),
#     ('Token.Keyword',                'FROM'),
#     ('Token.Text.Whitespace',        ' '),
#     ('Token.Name',                   'users'),
#     ('Token.Text.Whitespace',        ' '),
#     ('Token.Keyword',                'WHERE'),
#     ('Token.Text.Whitespace',        ' '),
#     ('Token.Name',                   'id'),
#     ('Token.Text.Whitespace',        ' '),
#     ('Token.Operator.Comparison',    '='),
#     ('Token.Text.Whitespace',        ' '),
#     ('Token.Literal.Number.Integer', '1'),
# ]
```

---

### `parsestream()` — parse a stream of statements

```python
import sqlparse_rs as sqlparse

stmts = list(sqlparse.parsestream("SELECT 1; INSERT INTO t VALUES (2); DELETE FROM t WHERE id = 3"))
# → 3 Statement objects

for s in stmts:
    print(s.get_type(), repr(str(s).strip()))
# SELECT  'SELECT 1;'
# INSERT  'INSERT INTO t VALUES (2);'
# DELETE  'DELETE FROM t WHERE id = 3'
```

Also accepts a file-like object:

```python
import io
stmts = list(sqlparse.parsestream(io.StringIO("SELECT 1; SELECT 2")))
```

---

### Sub-modules

`sqlparse_rs` ships its own `sql` and `tokens` modules — no `import sqlparse` ever needed:

```python
import sqlparse_rs

sqlparse_rs.sql       # AST node classes: Statement, Where, Identifier, ...
sqlparse_rs.tokens    # Token type constants: tokens.Keyword, tokens.Keyword.DML, ...
```

Token type constants work identically to `sqlparse.tokens`:

```python
from sqlparse_rs import tokens as T

T.Keyword              # → Token.Keyword
T.Keyword.DML          # → Token.Keyword.DML
T.DML in T.Keyword     # → True  (containment / hierarchy)
T.Newline in T.Whitespace  # → True
T.Literal.Number.Integer   # → Token.Literal.Number.Integer
```

AST node classes from `sqlparse_rs.sql`:

```python
from sqlparse_rs import sql

sql.Token          # leaf token
sql.TokenList      # base group
sql.Statement      # top-level statement
sql.Identifier     # bare name, quoted name, name AS alias
sql.IdentifierList # comma-separated identifiers
sql.Where          # WHERE ... clause
sql.Comparison     # lhs <op> rhs
sql.Function       # name(...)
sql.Parenthesis    # (...)
sql.Case           # CASE...END
sql.Comment        # comment block
sql.Begin          # BEGIN...END block
```

## Independence by default (no patching)

The two engines are separate packages with the same API. By default they
never interact — `import sqlparse_rs` does not import, modify, or patch
`sqlparse`, and `sqlparse` keeps running the genuine pure-Python engine.
This is pinned by `tests/test_independence.py` (subprocess-isolated).

```python
import sqlparse        # pure-Python engine — untouched, still the oracle
import sqlparse_rs     # Rust engine — fully independent

# side-by-side comparison works out of the box:
assert ([(str(t), v) for t, v in sqlparse.lexer.tokenize(sql)]
        == list(sqlparse_rs.tokenize(sql)))

# drop-in mode is an EXPLICIT, user-decided action:
sqlparse_rs.patch()    # redirect sqlparse.tokenize/parse/format → Rust
...                    # run foreign code that does `import sqlparse`
sqlparse_rs.unpatch()  # restore the original pure-Python engine exactly
```

`patch()` is idempotent, is a no-op when `sqlparse` is absent, and is
disabled by the `SQLPARSE_RS_NO_PATCH=1` environment variable. The Python
`sqlparse` package itself is only a development/benchmark dependency here
— it serves as the correctness oracle for the differential corpus and as
the baseline in every benchmark.

---

## Repo layout

```
pyproject.toml                maturin build config (mixed Rust/Python layout)
python/sqlparse_rs/
  __init__.py               re-exports tokenize/split/parse/format/parsestream + explicit patch/unpatch (no import side effects)
  _patch.py                 EXPLICIT opt-in patch()/unpatch()/is_patched()/disabled() — never automatic
  _stream.py                native parsestream() — StatementSplitter + Rust grouping
  sql.py                    pure-Python AST node classes (no sqlparse dep)
  tokens.py                 pure-Python _TokenType hierarchy (no sqlparse dep)
crates/sqlparse-core/src/
  ttype.rs                  &'static str token type constants
  keywords.rs               GENERATED 803-entry keyword map + binary search
  lexer.rs                  50-rule character scanner; zero-copy slices; O(n)
  spans.rs                  find_delimited_spans prepass ($-quotes, /* */ hints)
  splitter.rs               statement_splitter.py port; state machine
  lib.rs                    pub use lexer::tokenize, splitter::split
crates/sqlparse-py/src/lib.rs   PyO3 glue; exposes tokenize()+split()+parse()+format()
tests/
  test_compat.py            45 compatibility tests vs sqlparse
  test_native.py            22 standalone tests (no sqlparse dep)
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

# All Python tests (73 total)
pytest tests/ -v

# Single Python test
pytest tests/test_compat.py::test_split_begin_end_not_split -v

# Standalone tests (no sqlparse required)
pytest tests/test_native.py -v

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

## Implementation notes & known edges

### Covered by tests — non-obvious sqlparse quirks

- **Comments consume their trailing newline** — `m.group()` includes the terminator. `#` is only a
  comment when followed by a space; `--+` and `# +` are the MySQL/hint variants.
- **Number rules backtrack**: `1.5x` → `Float "1."` + `Name "5x"`; `12abc` → `Integer "1"` + `Name "2abc"`.
- **Unclosed strings do not swallow to EOF** — the opening quote becomes `Error`, the rest lexes normally.
- **Word continuation includes `$` and `#`**: `a#>>b` is `Name "a#"` + operator `>>`.
- **Rule 13 fires before the word rule** — `CASE|IN|VALUES|USING|FROM|AS` are always plain `Keyword`,
  never `Name`, regardless of context.
- **Rule 16 (lookbehind) is active**: `t.select` → the `select` part is `Token.Name`, not `Keyword`.
- **`split()` strips every statement**; `CASE...END` does NOT protect `;` — only `BEGIN...END` blocks do.
- **`BEGIN TRANSACTION` / `BEGIN WORK`** must not open a block — only bare `BEGIN` does (issue826).

### Limitations

- **`reindent_aligned` and `right_margin` format options are unimplemented** — both return the SQL
  unchanged. This matches real sqlparse too; they are no-ops in both implementations.
- **Unicode:** Rust uses `char::is_alphanumeric` / `char::is_numeric` to approximate Python `re`
  `\w`/`\d` semantics. Common Unicode identifiers (`中文`, `café`, `ß`) work correctly; exotic NFD
  combining marks may produce different token boundaries on malformed input.

## License

BSD-3-Clause; see LICENSE. Derivative work of
[python-sqlparse](https://github.com/andialbrecht/sqlparse) (BSD-3-Clause).
