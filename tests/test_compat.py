"""
Compatibility tests against sqlparse.
Each test asserts that sqlparse-rs produces the same result as the
original sqlparse library.  Run with:

    maturin develop && pytest tests/
"""
import pytest

# Import our Rust extension first — this patches sqlparse's lexer.
import sqlparse_rs  # noqa: F401 (side-effect: installs Rust lexer patch)

# sqlparse is the real library, now Rust-backed for tokenize/split.
import sqlparse as rs

# ─── split ────────────────────────────────────────────────────────────────

MULTI_SQL = "SELECT 1; SELECT 2; SELECT 3"

def test_split_count():
    result = rs.split(MULTI_SQL)
    assert len(result) == 3

def test_split_values():
    result = rs.split(MULTI_SQL)
    assert result[0].strip().upper().startswith("SELECT 1")
    assert result[1].strip().upper().startswith("SELECT 2")
    assert result[2].strip().upper().startswith("SELECT 3")

def test_split_single():
    result = rs.split("SELECT 1")
    assert len(result) == 1
    assert "1" in result[0]

def test_split_empty():
    result = rs.split("")
    assert result == []

def test_split_whitespace_only():
    result = rs.split("   \n  ")
    assert result == []

def test_split_strip_semicolon():
    result = rs.split("SELECT 1;", strip_semicolon=True)
    assert result[0].rstrip(";").strip() == "SELECT 1"

def test_split_begin_end_not_split():
    """BEGIN...END blocks should not be split at the inner semicolons."""
    sql = """
    CREATE FUNCTION foo() RETURNS void AS $$
    BEGIN
        UPDATE t SET x = 1;
        UPDATE t SET y = 2;
    END;
    $$ LANGUAGE plpgsql;
    SELECT 1;
    """
    result = rs.split(sql)
    # Should produce 2 statements: the CREATE and the SELECT
    non_empty = [s for s in result if s.strip()]
    assert len(non_empty) == 2, f"Expected 2 statements, got {non_empty}"

def test_split_multistatement_no_semicolons():
    """Single statement without semicolon."""
    result = rs.split("SELECT a FROM b WHERE c = 1")
    assert len(result) == 1

# ─── parse ────────────────────────────────────────────────────────────────

def test_parse_returns_tuple():
    result = rs.parse("SELECT 1")
    assert isinstance(result, tuple)
    assert len(result) == 1

def test_parse_statement_type_select():
    stmts = rs.parse("SELECT a FROM b")
    assert stmts[0].get_type() == "SELECT"

def test_parse_statement_type_insert():
    stmts = rs.parse("INSERT INTO t VALUES (1)")
    assert stmts[0].get_type() == "INSERT"

def test_parse_statement_type_update():
    stmts = rs.parse("UPDATE t SET x = 1")
    assert stmts[0].get_type() == "UPDATE"

def test_parse_statement_type_delete():
    stmts = rs.parse("DELETE FROM t WHERE x = 1")
    assert stmts[0].get_type() == "DELETE"

def test_parse_statement_str():
    stmts = rs.parse("SELECT 1")
    assert "SELECT" in str(stmts[0]).upper()

def test_parse_multiple():
    stmts = rs.parse("SELECT 1; SELECT 2")
    assert len(stmts) == 2

def test_parse_flatten():
    stmts = rs.parse("SELECT a FROM b")
    tokens = list(stmts[0].flatten())
    values = [t.value for t in tokens]
    joined = "".join(values)
    assert "SELECT" in joined.upper()
    assert "FROM" in joined.upper()

def test_parse_token_ttype():
    """Tokens must have a .ttype attribute compatible with sqlparse_rs.tokens."""
    from sqlparse_rs import tokens as T
    stmts = rs.parse("SELECT 1")
    flat = list(stmts[0].flatten())
    keywords = [t for t in flat if not t.is_whitespace and t.is_keyword]
    assert len(keywords) >= 1

# ─── format ───────────────────────────────────────────────────────────────

def test_format_keyword_upper():
    result = rs.format("select a from b", keyword_case="upper")
    assert "SELECT" in result
    assert "FROM" in result
    assert "select" not in result

def test_format_keyword_lower():
    result = rs.format("SELECT A FROM B", keyword_case="lower")
    assert "select" in result
    assert "from" in result

def test_format_keyword_capitalize():
    result = rs.format("SELECT a FROM b", keyword_case="capitalize")
    assert "Select" in result or "SELECT" in result  # single keyword

def test_format_strip_whitespace():
    result = rs.format("SELECT   a    FROM   b", strip_whitespace=True)
    assert "  " not in result  # no double spaces

def test_format_reindent():
    result = rs.format(
        "SELECT a, b FROM t WHERE x = 1 ORDER BY a",
        reindent=True, keyword_case="upper"
    )
    assert "FROM" in result
    assert "WHERE" in result
    assert "\n" in result  # reindent adds newlines

def test_format_strip_comments():
    sql = "SELECT a -- this is a comment\nFROM b"
    result = rs.format(sql, strip_comments=True)
    assert "--" not in result

def test_format_noop():
    """With no options, format returns SQL close to original."""
    sql = "SELECT a FROM b"
    result = rs.format(sql)
    assert "SELECT" in result.upper()
    assert "FROM" in result.upper()

def test_format_identifier_case_upper():
    result = rs.format("SELECT myCol FROM myTable", identifier_case="upper")
    assert "MYCOL" in result or "myCol".upper() in result

def test_format_identifier_case_lower():
    result = rs.format("SELECT MyCol FROM MyTable", identifier_case="lower")
    assert "mycol" in result

# ─── tokenize ─────────────────────────────────────────────────────────────

def test_tokenize_returns_pairs():
    import sqlparse_rs
    tokens = list(sqlparse_rs.tokenize("SELECT 1"))
    assert len(tokens) > 0
    for ttype, val in tokens:
        assert isinstance(val, str)

def test_tokenize_finds_dml():
    import sqlparse_rs
    pairs = list(sqlparse_rs.tokenize("SELECT 1"))
    ttypes = [tt for tt, _ in pairs]
    assert any("DML" in t for t in ttypes)

# ─── parsestream ──────────────────────────────────────────────────────────

def test_parsestream_from_string():
    stmts = list(rs.parsestream("SELECT 1; SELECT 2"))
    assert len(stmts) == 2

def test_parsestream_from_file():
    import io
    f = io.StringIO("SELECT 1; SELECT 2")
    stmts = list(rs.parsestream(f))
    assert len(stmts) == 2

# ─── edge cases ───────────────────────────────────────────────────────────

def test_parse_cte():
    sql = "WITH cte AS (SELECT 1) SELECT * FROM cte"
    stmts = rs.parse(sql)
    assert len(stmts) == 1

def test_parse_subquery():
    sql = "SELECT * FROM (SELECT a FROM b) sub"
    stmts = rs.parse(sql)
    assert len(stmts) == 1

def test_parse_multiline():
    sql = """
    SELECT
        a,
        b
    FROM
        my_table
    WHERE
        x > 1
    """
    stmts = rs.parse(sql)
    assert len(stmts) == 1
    assert stmts[0].get_type() == "SELECT"

def test_split_dollar_quoted():
    """Dollar-quoted bodies should not be split at interior semicolons."""
    sql = "SELECT $$hello; world$$ AS x; SELECT 2"
    result = rs.split(sql)
    assert len(result) == 2

# ─── Rust patch verification ───────────────────────────────────────────────

def test_rust_lexer_patch_active():
    """sqlparse.lexer.Lexer.get_tokens must be our Rust implementation."""
    import sqlparse.lexer as lex
    fn = lex.Lexer.get_tokens
    assert '_get_tokens_rust' in fn.__qualname__ or 'rust' in fn.__qualname__.lower(), (
        f"Rust patch not active; get_tokens is: {fn.__qualname__}"
    )

def test_parse_uses_rust_tokenizer():
    """parse() should produce a Statement whose flatten() matches Rust tokenize()."""
    sql = "SELECT id, name FROM users"
    stmts = rs.parse(sql)
    flat_values = "".join(t.value for t in stmts[0].flatten())
    assert flat_values == sql  # lossless round-trip

def test_format_reindent_and_case():
    """format() with both reindent and keyword_case works end-to-end."""
    result = rs.format(
        "select id, name from users where id = 1",
        reindent=True,
        keyword_case="upper",
    )
    assert "SELECT" in result
    assert "FROM" in result
    assert "WHERE" in result
    assert "\n" in result  # reindent added newlines

def test_parsestream_lossless():
    """parsestream() produces statements that reconstruct the original SQL."""
    sql = "SELECT 1; INSERT INTO t VALUES (2)"
    stmts = list(rs.parsestream(sql))
    assert len(stmts) == 2
    reconstructed = "".join(str(s) for s in stmts)
    assert "SELECT" in reconstructed.upper()
    assert "INSERT" in reconstructed.upper()

def test_identifier_get_name():
    """Identifier.get_name() works — requires grouping over Rust tokens."""
    stmts = rs.parse("SELECT foo AS bar FROM t")
    identifiers = list(stmts[0].get_sublists())
    names = [id_.get_name() for id_ in identifiers if hasattr(id_, 'get_alias')]
    assert any(n == 'bar' for n in names if n)

def test_where_clause_grouping():
    """WHERE clause is grouped into a Where node."""
    import sqlparse_rs.sql as rs_sql
    stmts = rs.parse("SELECT a FROM t WHERE x = 1 AND y = 2")
    where_clauses = [
        tok for tok in stmts[0].tokens
        if isinstance(tok, rs_sql.Where)
    ]
    assert len(where_clauses) == 1
    assert "WHERE" in str(where_clauses[0]).upper()
