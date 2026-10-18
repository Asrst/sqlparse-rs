---
name: sqlparse-compat
description: Use when implementing or testing drop-in compatibility with the sqlparse 0.6.0 Python API — covers the exact public surface, token type hierarchy, edge cases discovered from source reading, and the differential testing approach.
---

# sqlparse 0.6.0 Compatibility Reference

## Public API surface (must match exactly)

```python
import sqlparse

# 1. parse — returns tuple of Statement objects
stmts: tuple[Statement, ...] = sqlparse.parse(sql: str, encoding=None)

# 2. parsestream — generator variant
gen: Generator[Statement] = sqlparse.parsestream(stream: str|IO, encoding=None)

# 3. format — returns formatted string
out: str = sqlparse.format(sql: str, encoding=None, **options)

# 4. split — returns list of raw strings (trimmed)
parts: list[str] = sqlparse.split(sql: str, encoding=None, strip_semicolon=False)
```

## format() options (all must be supported)

| Option | Type | Default | Notes |
|--------|------|---------|-------|
| `keyword_case` | `'upper'|'lower'|'capitalize'|None` | `None` | |
| `identifier_case` | same | `None` | Skips quoted identifiers |
| `strip_comments` | bool | `False` | Preserves SQL hints (`/*+`) |
| `strip_whitespace` | bool | `False` | Auto-enabled by `reindent` |
| `reindent` | bool | `False` | Newline before FROM/WHERE/JOIN etc. |
| `reindent_aligned` | bool | `False` | Aligned-column variant |
| `indent_width` | int | `2` | Must be ≥ 1 |
| `indent_tabs` | bool | `False` | Sets `indent_char='\t'` |
| `indent_after_first` | bool | `False` | |
| `indent_columns` | bool | `False` | Forces `reindent=True` |
| `wrap_after` | int | `0` | 0 = no wrapping |
| `comma_first` | bool | `False` | |
| `compact` | bool | `False` | |
| `use_space_around_operators` | bool | `False` | |
| `truncate_strings` | int|None | `None` | Must be > 1 |
| `truncate_char` | str | `'[...]'` | |
| `right_margin` | int|None | `None` | Must be > 10 |
| `output_format` | `'sql'|'python'|'php'|None` | `None` | |

## Token class contract

Every token returned by `parse()` / `flatten()` must have:

```python
tok.ttype       # sqlparse.tokens._TokenType or None (for groups)
tok.value       # str — original text, never modified
tok.is_group    # bool — True for TokenList subclasses
tok.is_whitespace  # bool
tok.is_keyword  # bool
tok.normalized  # str — value.upper() if keyword, else value
tok.parent      # parent TokenList or None

tok.flatten()   # generator/list of leaf Token objects
tok.match(ttype, values, regex=False)  # bool
```

## Token type hierarchy (sqlparse.tokens)

```
Token
├── Token.Text.Whitespace
│   └── Token.Text.Whitespace.Newline
├── Token.Error
├── Token.Keyword           ← general keywords
│   ├── Token.Keyword.DML   ← SELECT INSERT UPDATE DELETE REPLACE MERGE
│   ├── Token.Keyword.DDL   ← CREATE ALTER DROP ...
│   ├── Token.Keyword.CTE   ← WITH
│   ├── Token.Keyword.Order ← ASC DESC
│   └── Token.Keyword.TZCast
├── Token.Name
│   ├── Token.Name.Placeholder
│   └── Token.Name.Builtin  ← INT VARCHAR etc.
├── Token.Literal
│   ├── Token.Literal.String.Single   ← '...'
│   ├── Token.Literal.String.Symbol   ← "..." `...` [...]
│   └── Token.Literal.Number.Integer / .Float / .Hexadecimal
├── Token.Punctuation       ← ; , ( ) [ ] . :
├── Token.Operator
│   └── Token.Operator.Comparison  ← = < > != <> <= >=
├── Token.Wildcard          ← *
├── Token.Comment.Single    ← -- ...
└── Token.Comment.Multiline ← /* ... */
```

`in` containment: `T.Keyword.DML in T.Keyword` is `True` (prefix match on tuple).

## Statement class contract

```python
stmt.get_type()  # 'SELECT'|'INSERT'|'UPDATE'|'DELETE'|'CREATE'|...|'UNKNOWN'
str(stmt)        # original SQL text (lossless round-trip)
```

## split() behaviour quirks (verified from source)

- Each result is `.trim()`-ped — leading/trailing whitespace stripped
- Empty strings are filtered out
- `CASE...END` does **not** protect inner `;` — only `BEGIN...END` blocks do
- `GO` (MSSQL) acts as a statement terminator
- `BEGIN TRANSACTION` / `BEGIN WORK` must NOT open a block (issue826)
- Semicolons inside `$$...$$` dollar-quoted strings must be ignored

## Lexer edge cases (from README "What was hard")

- Comments CONSUME their trailing newline — the value includes `\n`
- `#` is a comment only when followed by a space (`# comment`); `#>>` is an operator
- `--+` and `# +` are MySQL/PG hint markers (`Token.Comment.*.Hint`)
- Number rules backtrack: `1.5x` → `Float "1."` + `Name "5x"`; `12abc` → `Integer "1"` + `Name "2abc"`
- Unclosed strings produce `Error` token for the quote char, then lex normally
- Word chars include `$` and `#`: `a#>>b` is `Name "a#"` + `>>`
- Rule 13 fires before the word rule: `FROM` inside a regex match → plain `Keyword`, not `Keyword.DML`
- Lookbehind rule 16 IS active: `t.select` → the `select` part is `Token.Name`

## Differential testing approach

The project uses golden-file differential testing:

```bash
tools/oracle.py tokenize < corpus/sql/foo.sql > corpus/goldens.txt   # generate referee output
cargo test                # Rust tests compare against goldens
pytest tests/             # Python wrapper tests compare against sqlparse
```

Golden files committed at `corpus/goldens*.txt` are the source of truth.
When sqlparse behaviour is ambiguous, `tools/oracle.py` is the referee — not intuition.

## AST node types (sqlparse.sql)

| Class | Triggered by |
|-------|-------------|
| `Statement` | Top-level wrapper for each statement |
| `Identifier` | Bare name, quoted name, `name AS alias`, `schema.table` |
| `IdentifierList` | Comma-separated identifiers |
| `Parenthesis` | `(...)` |
| `Where` | `WHERE ... <terminator>` |
| `Comparison` | `lhs <op> rhs` |
| `Function` | `name(...)` |
| `Comment` | Block of comment tokens |
| `Case` | `CASE...END` |
| `Operation` | Arithmetic `a + b` |
| `Values` | `VALUES (...)` |
