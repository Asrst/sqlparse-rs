"""
Tests for sqlparse_rs.sql and sqlparse_rs.tokens without any sqlparse dependency.
These exercise the pure-Python implementation shipped inside sqlparse_rs.
"""
import pytest

import sqlparse_rs
import sqlparse_rs.tokens as T
import sqlparse_rs.sql as sql_mod


# ── tokens module ──────────────────────────────────────────────────────────────

def test_tokens_hierarchy():
    """_TokenType containment works correctly."""
    assert T.DML in T.Keyword
    assert T.Keyword in T.Keyword
    assert T.Keyword not in T.DML
    assert T.Newline in T.Whitespace
    assert T.Whitespace not in T.Newline


def test_tokens_equality():
    """Module-level aliases equal the traversal path."""
    assert T.DML == T.Token.Keyword.DML
    assert T.Whitespace == T.Token.Text.Whitespace
    assert T.Newline == T.Token.Text.Whitespace.Newline


def test_tokens_repr():
    """str/repr output matches sqlparse convention."""
    assert str(T.DML) == "Token.Keyword.DML"
    assert str(T.Whitespace) == "Token.Text.Whitespace"
    assert repr(T.DML) == "Token.Keyword.DML"


def test_tokens_lazy_child():
    """Accessing a new attribute lazily creates a child _TokenType."""
    custom = T.Token.Custom.Thing
    assert str(custom) == "Token.Custom.Thing"
    assert custom in T.Token.Custom
    assert custom in T.Token


# ── sql.Token ──────────────────────────────────────────────────────────────────

def test_token_basic():
    tok = sql_mod.Token(T.Keyword.DML, "SELECT")
    assert tok.value == "SELECT"
    assert tok.ttype is T.DML
    assert tok.is_keyword
    assert not tok.is_whitespace
    assert tok.normalized == "SELECT"


def test_token_whitespace():
    tok = sql_mod.Token(T.Text.Whitespace, " ")
    assert tok.is_whitespace
    assert not tok.is_keyword


def test_token_match():
    tok = sql_mod.Token(T.Keyword.DML, "SELECT")
    assert tok.match(T.Keyword.DML, "SELECT")
    assert tok.match(T.Keyword.DML, "select")  # case-insensitive for keywords
    assert not tok.match(T.Keyword.DML, "INSERT")


def test_token_flatten():
    tok = sql_mod.Token(T.Keyword.DML, "SELECT")
    assert list(tok.flatten()) == [tok]


def test_token_str():
    tok = sql_mod.Token(T.Keyword.DML, "SELECT")
    assert str(tok) == "SELECT"


# ── sql.TokenList ──────────────────────────────────────────────────────────────

def test_tokenlist_basic():
    tokens = [
        sql_mod.Token(T.Keyword.DML, "SELECT"),
        sql_mod.Token(T.Text.Whitespace, " "),
        sql_mod.Token(T.Literal.Number.Integer, "1"),
    ]
    tl = sql_mod.TokenList(tokens)
    assert str(tl) == "SELECT 1"
    assert tl.is_group


def test_tokenlist_flatten():
    tokens = [
        sql_mod.Token(T.Keyword.DML, "SELECT"),
        sql_mod.Token(T.Text.Whitespace, " "),
        sql_mod.Token(T.Literal.Number.Integer, "1"),
    ]
    tl = sql_mod.TokenList(tokens)
    flat = list(tl.flatten())
    assert len(flat) == 3
    assert flat[0].value == "SELECT"


def test_tokenlist_parent_set():
    tok = sql_mod.Token(T.Keyword.DML, "SELECT")
    tl = sql_mod.TokenList([tok])
    assert tok.parent is tl


def test_tokenlist_token_first():
    tokens = [
        sql_mod.Token(T.Text.Whitespace, " "),
        sql_mod.Token(T.Keyword.DML, "SELECT"),
    ]
    tl = sql_mod.TokenList(tokens)
    first = tl.token_first(skip_ws=True)
    assert first.value == "SELECT"


# ── sql.Statement ──────────────────────────────────────────────────────────────

def test_statement_get_type_select():
    stmts = sqlparse_rs.parse("SELECT 1")
    assert stmts[0].get_type() == "SELECT"


def test_statement_get_type_insert():
    stmts = sqlparse_rs.parse("INSERT INTO t VALUES (1)")
    assert stmts[0].get_type() == "INSERT"


def test_statement_get_type_update():
    stmts = sqlparse_rs.parse("UPDATE t SET x = 1")
    assert stmts[0].get_type() == "UPDATE"


def test_statement_get_type_delete():
    stmts = sqlparse_rs.parse("DELETE FROM t WHERE x = 1")
    assert stmts[0].get_type() == "DELETE"


def test_statement_str_roundtrip():
    sql = "SELECT a FROM b"
    stmts = sqlparse_rs.parse(sql)
    assert str(stmts[0]) == sql


def test_statement_flatten_lossless():
    sql = "SELECT id FROM users"
    stmts = sqlparse_rs.parse(sql)
    joined = "".join(t.value for t in stmts[0].flatten())
    assert joined == sql


# ── sql.Where ─────────────────────────────────────────────────────────────────

def test_where_node_type():
    stmts = sqlparse_rs.parse("SELECT a FROM t WHERE x = 1")
    where_nodes = [tok for tok in stmts[0].tokens if isinstance(tok, sql_mod.Where)]
    assert len(where_nodes) == 1
    assert "WHERE" in str(where_nodes[0]).upper()


# ── sql.Identifier ────────────────────────────────────────────────────────────

def test_identifier_get_name():
    stmts = sqlparse_rs.parse("SELECT foo AS bar FROM t")
    identifiers = list(stmts[0].get_sublists())
    aliases = [i.get_alias() for i in identifiers if hasattr(i, "get_alias")]
    assert "bar" in aliases


# ── tokens module as drop-in for sqlparse.tokens ──────────────────────────────

def test_tokens_droping_with_in_operator():
    """ttype in parent_ttype behaves identically to sqlparse."""
    assert T.Token.Keyword.DML in T.Token.Keyword
    assert T.Token.Keyword.DDL in T.Token.Keyword
    assert T.Token.Keyword in T.Token
    assert T.Token.Keyword.DML not in T.Token.Keyword.DDL
