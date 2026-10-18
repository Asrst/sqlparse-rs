# sqlparse-rs

A Rust port of [python-sqlparse](https://github.com/andialbrecht/sqlparse) 0.6.0, exposed to Python via [PyO3](https://github.com/PyO3/pyo3). It is a **complete, standalone SQL engine** with the same public API as sqlparse — no runtime dependency on the Python library.

`import sqlparse_rs` never touches sqlparse. The two packages install side by side, run independently, and you can compare them directly in the same interpreter. Redirecting sqlparse to the Rust engine is a deliberate opt-in: `sqlparse_rs.patch()` and `sqlparse_rs.unpatch()`.

---

## Why

sqlparse is widely used but it is slow on large inputs. A 8.8 MB pg_dump takes ~13 seconds to tokenize. A 10 MB file with unclosed dollar-quoted strings never finishes (timeout after 120 s). The Python regex engine has no escape from that.

This project rewrites the hot paths in Rust: a hand-written 50-rule character scanner (O(n), no backtracking), a state-machine statement splitter, Rust grouping and a fully Rust formatter. The output is byte-identical to sqlparse 0.6.0 on every input in the corpus.

---

## Status

| Component | Status |
|-----------|--------|
| Lexer / tokenizer | Complete, byte-identical to sqlparse 0.6.0 |
| Statement splitter (`split()`) | Complete, handles BEGIN/END blocks, GO, dollar-quotes |
| `parse()` / `format()` / `parsestream()` | Complete, Rust tokenize + grouping + formatter |
| `sql` / `tokens` sub-modules | Complete, zero sqlparse runtime dependency |
| `patch()` / `unpatch()` compat layer | Complete, explicit and reversible, never automatic |

84 tests pass (23 Rust, 61 Python). Parity verified against a 47-file corpus covering 9 SQL dialects, adversarial inputs and issue regressions.

---

## Benchmarks

All numbers are end-to-end through the Python API. Equal-work validation (token count + sha256 stream checksum) is asserted before any timing is reported; a mismatch voids the run. Numbers are min-of-3, 1 warmup discarded.

### Batch workloads

| Workload | sqlparse 0.6.0 | sqlparse-rs | Speedup |
|----------|---------------|-------------|---------|
| Tokenize 8.8 MB dump (2.79M tokens) | 12.6 s (0.70 MB/s) | 2.3 s (3.82 MB/s) | **5.5x** |
| Split 8.8 MB dump (7,600 statements) | 17.3 s | 2.0 s | **8.7x** |
| Fingerprint 100k queries (query-log) | 25.9 s | 5.0 s | **5.1x** |

Pure Rust scanner (no PyO3 boundary): 1.35 MB corpus, 103,800 tokens: Python 0.777 s vs Rust 0.114 s, **~6.8x**.

The end-to-end numbers are lower than the pure-Rust ones because `tokenize()` has to materialise 2.79 million Python tuples. `split()` returns 7,600 objects and shows the bigger win for exactly that reason. The next lever is a zero-copy iterator return (future work).

### Large and pathological inputs

These tests measure what happens when the input is adversarial. Each run uses a subprocess with a 120 s timeout; identical bytes go to both engines.

| Input type | Size | sqlparse 0.6.0 | sqlparse-rs |
|------------|------|---------------|-------------|
| Dollar-quoted string, never closed | 1 MB | 8.01 s | 1.86 s |
| Dollar-quoted string, never closed | 10 MB | **never finished (> 120 s)** | 18.6 s |
| SQL comments only | 10 MB | 0.81 s | 0.27 s |

The comment-only row is the fairness control: both engines are linear on that input and the gap stays around 3x, matching the benign batch workloads above. The 10 MB dollar-quote case is where the Python regex engine hits a wall; Rust finishes in linear time.

Reproduce:

```bash
python benchmarks/bench.py
python benchmarks/scaling.py
cargo run --release -p sqlparse-core --bin bench < corpus/inputs.bin
```

Raw data in `benchmarks/results.csv` and `benchmarks/scaling.csv`. Chart in `benchmarks/scaling.png`.

---

## Installation

```bash
# Rust toolchain (skip if already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

pip install maturin
maturin develop --release
```

Requires **Rust 1.75+** and **Python 3.8+**.

---

## Usage

### Drop-in replacement

```python
import sqlparse_rs as sqlparse
```

Every sqlparse function and sub-module is available on `sqlparse_rs`. Most code needs no other change.

### split()

```python
import sqlparse_rs as sqlparse

sqlparse.split("SELECT 1; SELECT 2; SELECT 3")
# ['SELECT 1;', 'SELECT 2;', 'SELECT 3']
```

Dollar-quoted strings are handled correctly (inner semicolons are never split boundaries):

```python
sqlparse.split("SELECT $$hello; world$$ AS x; SELECT 2")
# ['SELECT $$hello; world$$ AS x;', 'SELECT 2']
```

BEGIN...END blocks preserve their inner semicolons:

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
# 2 statements: the whole CREATE FUNCTION body, then SELECT 1
```

### parse()

```python
import sqlparse_rs as sqlparse

stmts = sqlparse.parse("SELECT id, name FROM users WHERE id = 1")
stmt  = stmts[0]

stmt.get_type()   # 'SELECT'
str(stmt)         # 'SELECT id, name FROM users WHERE id = 1'
```

Works for all DML types:

```python
sqlparse.parse("SELECT a FROM b")[0].get_type()          # 'SELECT'
sqlparse.parse("INSERT INTO t VALUES (1)")[0].get_type() # 'INSERT'
sqlparse.parse("UPDATE t SET x=1")[0].get_type()         # 'UPDATE'
sqlparse.parse("DELETE FROM t WHERE id=1")[0].get_type() # 'DELETE'
```

Flatten to individual tokens:

```python
for tok in sqlparse.parse("SELECT id FROM users WHERE id = 1")[0].flatten():
    print(tok.ttype, repr(tok.value))

# Token.Keyword.DML 'SELECT'
# Token.Text.Whitespace ' '
# Token.Name 'id'
# ...
```

Access typed AST nodes:

```python
import sqlparse_rs.sql as sql

stmt = sqlparse_rs.parse("SELECT a FROM t WHERE x = 1 AND y = 2")[0]
where = [tok for tok in stmt.tokens if isinstance(tok, sql.Where)]
str(where[0])   # 'WHERE x = 1 AND y = 2'
```

CTEs and subqueries parse as a single statement:

```python
sqlparse_rs.parse(
    "WITH cte AS (SELECT id FROM users) SELECT * FROM cte WHERE id > 5"
)[0].get_type()
# 'SELECT'
```

### format()

```python
import sqlparse_rs as sqlparse

# Reindent and uppercase keywords
sqlparse.format(
    "select id, name from users where id = 1 order by id",
    reindent=True,
    keyword_case="upper",
)
# 'SELECT id, name\nFROM users\nWHERE id = 1\nORDER BY id'

# Strip extra whitespace
sqlparse.format("SELECT   a    FROM   b", strip_whitespace=True)
# 'SELECT a FROM b'

# Strip comments
sqlparse.format(
    "SELECT a -- comment\nFROM b /* block */",
    strip_comments=True,
    strip_whitespace=True,
)
# 'SELECT a FROM b'

# Lowercase keywords, uppercase identifiers
sqlparse.format("SELECT A FROM B WHERE X = 1", keyword_case="lower")
# 'select A from B where X = 1'
```

### tokenize()

Returns a list of `(token_type_string, value)` tuples, byte-identical to sqlparse:

```python
import sqlparse_rs as sqlparse

sqlparse.tokenize("SELECT id FROM users WHERE id = 1")
# [
#   ('Token.Keyword.DML',            'SELECT'),
#   ('Token.Text.Whitespace',        ' '),
#   ('Token.Name',                   'id'),
#   ('Token.Text.Whitespace',        ' '),
#   ('Token.Keyword',                'FROM'),
#   ('Token.Text.Whitespace',        ' '),
#   ('Token.Name',                   'users'),
#   ('Token.Text.Whitespace',        ' '),
#   ('Token.Keyword',                'WHERE'),
#   ('Token.Text.Whitespace',        ' '),
#   ('Token.Name',                   'id'),
#   ('Token.Text.Whitespace',        ' '),
#   ('Token.Operator.Comparison',    '='),
#   ('Token.Text.Whitespace',        ' '),
#   ('Token.Literal.Number.Integer', '1'),
# ]
```

### parsestream()

```python
import sqlparse_rs as sqlparse

for s in sqlparse.parsestream("SELECT 1; INSERT INTO t VALUES (2); DELETE FROM t WHERE id = 3"):
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

### tokens and sql sub-modules

```python
from sqlparse_rs import tokens as T

T.Keyword              # Token.Keyword
T.Keyword.DML          # Token.Keyword.DML
T.DML in T.Keyword     # True  (hierarchy containment)
T.Newline in T.Whitespace  # True
```

```python
from sqlparse_rs import sql

sql.Token          # leaf token
sql.TokenList      # base group
sql.Statement      # top-level statement
sql.Identifier     # bare name, quoted name, name AS alias
sql.IdentifierList # comma-separated identifiers
sql.Where          # WHERE clause
sql.Comparison     # lhs <op> rhs
sql.Function       # name(...)
sql.Parenthesis    # (...)
sql.Case           # CASE...END
sql.Comment        # comment block
sql.Begin          # BEGIN...END block
```

---

## Independence by default

`import sqlparse_rs` never imports, modifies, or patches sqlparse. Both packages live side by side in the same interpreter. This contract is tested in `tests/test_independence.py` using subprocess isolation.

```python
import sqlparse        # pure-Python engine, untouched
import sqlparse_rs     # Rust engine, independent

# Direct comparison works out of the box
py_tokens = [(str(t), v) for t, v in sqlparse.lexer.tokenize(sql)]
rs_tokens = list(sqlparse_rs.tokenize(sql))
assert py_tokens == rs_tokens

# Redirecting sqlparse to Rust is an explicit, user-driven action
sqlparse_rs.patch()    # sqlparse.tokenize / parse / format now use Rust
...
sqlparse_rs.unpatch()  # original pure-Python engine restored exactly
```

`patch()` is idempotent, a no-op when sqlparse is absent, and can be disabled with `SQLPARSE_RS_NO_PATCH=1`.

---

## Development

```bash
# Activate toolchain and venv (required in this devcontainer)
source "$HOME/.cargo/env"
source .venv/bin/activate

# Build debug .so and install into venv (run from repo root)
maturin develop

# Optimised build
maturin develop --release

# All Python tests
pytest tests/ -v

# Single test
pytest tests/test_compat.py::test_split_begin_end_not_split -v

# Standalone tests (no sqlparse required)
pytest tests/test_native.py -v

# All Rust tests
cargo test -p sqlparse-core

# Single Rust test
cargo test -p sqlparse-core test_dollar_quoted -- --nocapture

# Regenerate keyword table from installed sqlparse
make gen-keywords

# Regenerate golden files
make corpus goldens split-goldens

# Run parity gate
make check
```

---

## Repo layout

```
pyproject.toml                    maturin build config (mixed Rust/Python layout)
python/sqlparse_rs/
  __init__.py                     re-exports all public API; patch/unpatch explicit opt-in
  _patch.py                       patch() / unpatch() / is_patched() / disabled()
  _stream.py                      native parsestream()
  sql.py                          AST node classes (no sqlparse dependency)
  tokens.py                       token type hierarchy (no sqlparse dependency)
crates/sqlparse-core/src/
  ttype.rs                        &'static str token type constants
  keywords.rs                     GENERATED 803-entry keyword map + binary search
  lexer.rs                        50-rule character scanner; O(n), no backtracking
  spans.rs                        prepass for dollar-quotes and block comments
  splitter.rs                     statement splitter state machine
  grouping.rs                     AST grouping (Rust port of sqlparse grouping.py)
  formatter.rs                    SQL formatter (Rust port of sqlparse filters)
  lib.rs                          public API: tokenize, split, parse, format
crates/sqlparse-py/src/lib.rs     PyO3 bindings
tests/
  test_compat.py                  compatibility tests against sqlparse (patch-mode)
  test_native.py                  standalone tests, no sqlparse dependency
  test_independence.py            subprocess-isolated independence contract tests
  test_parity.py                  corpus and fuzz parity sweep
  test_wrapper.py                 Rust parity vs real sqlparse, byte-identical
corpus/sql/                       47 SQL fixture files (9 dialects + regressions)
corpus/goldens.txt                tokenize goldens (real sqlparse 0.6.0)
corpus/goldens-split.txt          split goldens
benchmarks/
  bench.py                        end-to-end batch benchmark
  scaling.py                      adversarial scaling sweep
  results.csv                     raw benchmark data
  scaling.csv                     adversarial scaling data
  scaling.png                     scaling chart
tools/
  oracle.py                       differential oracle (tokenize / split / words)
  gen_keywords.py                 regenerates keywords.rs from installed sqlparse
PORT_MATRIX.md                    generated spec of all 50 lexer rules
docs/DECISIONS.md                 architecture decisions log
```

---

## Implementation notes

### Tokenizer behaviour worth knowing

- `CASE`, `IN`, `VALUES`, `USING`, `FROM`, `AS` are always `Token.Keyword` regardless of context. You cannot shadow them with a column or table name in the token stream.
- A name immediately after a dot is always `Token.Name`. So `t.select` gives `Name "t"`, `Punctuation "."`, `Name "select"` — the reserved word is demoted by position.
- `#` is only a line comment when followed by a space (`# comment`). A bare `#` inside a word is a word character, so `a#b` is a single `Name` token.
- Number parsing can split unexpectedly: `1.5x` becomes `Float "1."` + `Name "5x"`, and `12abc` becomes `Integer "1"` + `Name "2abc"`.
- Unclosed string literals do not consume the rest of the file. The opening quote becomes a single `Error` token and lexing continues normally.
- Comments include their trailing newline in the token value.

### Splitter behaviour worth knowing

- `split()` always strips leading and trailing whitespace from each statement.
- `CASE...END` does **not** protect semicolons. Only `BEGIN...END` blocks do. So `SELECT CASE WHEN a THEN 1; ELSE 2; END;` splits into four separate statements.
- `BEGIN TRANSACTION` and `BEGIN WORK` do not open a block. Only a bare `BEGIN` on its own does.

### Limitations

- `reindent_aligned` and `right_margin` format options are not implemented. Both return the SQL unchanged (same behaviour as sqlparse itself).
- Unicode: Rust uses `char::is_alphanumeric` to approximate Python `\w` semantics. Common identifiers (`中文`, `café`, `ß`) work correctly; exotic NFD combining marks on malformed input may produce different token boundaries.

---

## License

BSD-3-Clause. See [LICENSE](LICENSE). Derivative work of [python-sqlparse](https://github.com/andialbrecht/sqlparse) (BSD-3-Clause).
