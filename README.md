# sqlparse-rs

A Rust port of [python-sqlparse](https://github.com/andialbrecht/sqlparse) 0.6.0 exposed to Python
via [PyO3](https://github.com/PyO3/pyo3) — a **drop-in replacement** with a single import.

## Status

| Tier | Component | Status |
|------|-----------|--------|
| 1 | Lexer / tokenizer | ✅ Complete — 6.8× faster, byte-identical to sqlparse 0.6.0 |
| 1 | Statement splitter (`split()`) | ✅ Complete — BEGIN/END blocks, GO, dollar-quotes |
| 2 | Drop-in `import sqlparse` shim | ✅ Complete — Rust lexer patched into sqlparse's engine |
| 2 | `parse()` / `format()` / `parsestream()` | ✅ Complete — Rust tokens, grouping, and formatter |
| 3 | Full Rust grouping + formatter | ✅ Complete — Rust AST grouping + formatter; no Python grouping engine |

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

# Python deps
pip install maturin sqlparse==0.6.0

# Build and install into current venv (run from repo root)
maturin develop --release
```

Requires Rust 1.75+, Python 3.8+.

> **Why is `sqlparse` required?** `sqlparse_rs` is an accelerator, not a full replacement of
> sqlparse's Python layer. `parse()` returns `sqlparse.sql.Statement` objects (Python classes
> from the real package), `format()` uses sqlparse's formatter infrastructure, and
> `import sqlparse_rs as sqlparse` re-exports sqlparse's sub-modules directly. The pure-Rust
> functions `tokenize()` and `split()` work without sqlparse, but everything else depends on it
> at runtime. `sqlparse==0.6.0` is a hard runtime dependency.

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
stmt = sqlparse.parse("SELECT a FROM t WHERE x = 1 AND y = 2")[0]

# WHERE clause is grouped into a Where node
where = [tok for tok in stmt.tokens if isinstance(tok, sqlparse.sql.Where)]
str(where[0])   # → 'WHERE x = 1 AND y = 2'

# Identifier alias
stmt = sqlparse.parse("SELECT foo AS bar FROM users")[0]
for id_ in stmt.get_sublists():
    print(id_.get_name(), id_.get_alias())
# bar  bar
# users  None
```

CTEs and subqueries work as a single statement:

```python
sqlparse.parse("WITH cte AS (SELECT id FROM users) SELECT * FROM cte WHERE id > 5")[0].get_type()
# → 'SELECT'

sqlparse.parse("SELECT * FROM (SELECT a FROM b) sub")[0].get_type()
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

All `sqlparse` sub-modules are re-exported — no `import sqlparse` ever needed:

```python
import sqlparse_rs as sqlparse

sqlparse.sql          # AST node classes: Statement, Where, Identifier, ...
sqlparse.tokens       # Token type constants: tokens.Keyword, tokens.Keyword.DML, ...
sqlparse.filters      # Filter pipeline
sqlparse.engine       # Grouping engine
sqlparse.formatter    # Formatter internals
sqlparse.keywords     # Keyword tables
sqlparse.lexer        # Lexer (Rust-patched)
sqlparse.utils        # Utility helpers
sqlparse.exceptions   # Exception classes
```

Example using token type constants:

```python
import sqlparse_rs as sqlparse

sqlparse.tokens.Keyword              # → Token.Keyword
sqlparse.tokens.Keyword.DML          # → Token.Keyword.DML
sqlparse.tokens.Literal.Number.Integer  # → Token.Literal.Number.Integer
```

---

## Repo layout

```
pyproject.toml                maturin build config (mixed Rust/Python layout)
python/sqlparse_rs/
  __init__.py               re-exports tokenize/split/parse/format + all sub-modules
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
  test_compat.py            45 compatibility tests vs sqlparse
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

- **`import sqlparse_rs` must happen before you need Rust to be active.** The patch replaces
  `sqlparse.parse` and `sqlparse.format` at import time, so any calls made after `import sqlparse_rs`
  use Rust even if `import sqlparse` ran first. Place it in `conftest.py`, your app `__init__.py`,
  or any entry-point — just not after another library has already cached a direct reference to the
  original functions.
- **`Lexer.add_keywords()` calls are silently ignored by the Rust scanner.** The Python `Lexer`
  instance still stores the entries, but the Rust scanner uses its compiled keyword table
  (`keywords.rs`). To add keywords permanently, regenerate `keywords.rs` with `make gen-keywords`.
- **`reindent_aligned` and `right_margin` format options are unimplemented** — both return the SQL
  unchanged. This matches real sqlparse too; they are no-ops in both implementations.
- **Unicode:** Rust uses `char::is_alphanumeric` / `char::is_numeric` to approximate Python `re`
  `\w`/`\d` semantics. Common Unicode identifiers (`中文`, `café`, `ß`) work correctly; exotic NFD
  combining marks may produce different token boundaries on malformed input.

## License

BSD-3-Clause; see LICENSE. Derivative work of
[python-sqlparse](https://github.com/andialbrecht/sqlparse) (BSD-3-Clause).
