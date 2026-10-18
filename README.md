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
| 3 | Own `sql` / `tokens` / `parsestream` | ✅ Complete — zero `sqlparse` runtime dependency |

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

When `sqlparse` is installed, `import sqlparse_rs` also patches its lexer so
`sqlparse.parse()` / `sqlparse.format()` use Rust automatically.

---

## Repo layout

```
pyproject.toml                maturin build config (mixed Rust/Python layout)
python/sqlparse_rs/
  __init__.py               re-exports tokenize/split/parse/format/parsestream; optional patch
  _patch.py                 optional monkey-patch of sqlparse.lexer.Lexer.get_tokens
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

## License

BSD-3-Clause; see LICENSE. Derivative work of
[python-sqlparse](https://github.com/andialbrecht/sqlparse) (BSD-3-Clause).
