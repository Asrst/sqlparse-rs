"""Pytest plugin (-p downstream_shim): alias `sqlparse` -> `sqlparse_rs`.

Injects a shim module for `sqlparse` (and `sqlparse.lexer`) whose FULL
surface — tokenize / split / parse / format — is served by sqlparse_rs.
tokenize yields REAL `sqlparse.tokens` _TokenType objects (resolved from the
dotted-path strings sqlparse_rs emits), so upstream assertions like
`tokens[0] == (T.Keyword.DML, 'select')` evaluate against the genuine type
objects.

Every failure under this shim is therefore a REAL divergence between
sqlparse_rs and sqlparse 0.6.0 — there is no un-ported surface left to
excuse.

Baseline runs without this plugin get the real sqlparse (no injection).
"""
import sys
import types

import sqlparse_rs


def _resolve_ttype(path, _cache={}):
    """'Token.Keyword.DML' -> the real _TokenType object."""
    from sqlparse import tokens as T
    obj = T.Token
    for part in path.split("."):
        key = (id(obj), part)
        if key not in _cache:
            _cache[key] = getattr(obj, part)
        obj = _cache[key]
    return obj


def _as_str(sql):
    """Accept file-like objects like python sqlparse does (it .read()s them)."""
    return sql.read() if hasattr(sql, "read") else sql


def _rs_tokenize_gen(sql):
    for t, v in sqlparse_rs.tokenize(_as_str(sql)):
        yield (_resolve_ttype(t), v)


def _install():
    import sqlparse.tokens as _tokens
    import sqlparse.exceptions as _exceptions
    try:
        import sqlparse.sql as _sql
    except Exception:  # grouping deps — only needed by parse tests
        _sql = None

    # Share the real sqlparse _TokenType singletons into sqlparse_rs.tokens
    # so identity checks (`token.ttype is T.Keyword`) hold across engines
    # for parse trees built from our sql classes. Same one-way sharing that
    # sqlparse_rs.patch() performs — the shim is an explicit opt-in tool and
    # cross-engine identity is part of the alias contract. (This used to be
    # done implicitly by sqlparse_rs.tokens at import time; it is now
    # explicit, per the independence-by-default architecture.)
    import sqlparse_rs.tokens as _rs_tokens
    for _name in dir(_tokens):
        if not _name.startswith("_"):
            setattr(_rs_tokens, _name, getattr(_tokens, _name))

    shim = types.ModuleType("sqlparse")
    shim.__version__ = getattr(sqlparse_rs, "__version__", "?") + "+rs-shim"
    shim.tokenize = _rs_tokenize_gen
    shim.split = lambda s, **k: sqlparse_rs.split(_as_str(s), **k)
    shim.parse = lambda s, **k: sqlparse_rs.parse(_as_str(s), **k)
    shim.format = lambda s, **k: sqlparse_rs.format(_as_str(s), **k)
    shim.parsestream = sqlparse_rs.parsestream
    shim.tokens = _tokens
    shim.exceptions = _exceptions
    if _sql is not None:
        # Serve sqlparse_rs's OWN sql classes: parse() returns objects built
        # from them, so isinstance() checks in downstream tests evaluate
        # against the classes that actually produced the tree.
        import sqlparse_rs.sql as _rs_sql
        shim.sql = _rs_sql

    lexer_shim = types.ModuleType("sqlparse.lexer")
    lexer_shim.tokenize = _rs_tokenize_gen

    sys.modules["sqlparse"] = shim
    sys.modules["sqlparse.lexer"] = lexer_shim


_install()
