# sqlparse_rs._patch
# Monkey-patches sqlparse's lexer AND top-level parse/format to use Rust.
# Safe to call multiple times (idempotent).

_INSTALLED = False


def install():
    """Patch sqlparse so tokenize/parse/format all call Rust instead of Python."""
    global _INSTALLED
    if _INSTALLED:
        return

    try:
        import sqlparse.lexer as _lex_mod
        import sqlparse.tokens as _T
        import sqlparse as _sqlparse
    except ImportError:
        # sqlparse not installed — nothing to patch
        return

    from sqlparse_rs.sqlparse_rs import (
        tokenize as _rust_tokenize_raw,
        parse    as _rust_parse,
        format   as _rust_format,
    )

    # ── Lexer patch (unchanged from before) ──────────────────────────────

    # Cache ttype-string → _TokenType objects to avoid repeated attribute traversal.
    _ttype_cache: dict = {}

    def _resolve(ttype_str: str):
        t = _ttype_cache.get(ttype_str)
        if t is not None:
            return t
        cur = _T.Token
        for part in ttype_str.split('.')[1:]:  # skip leading "Token"
            cur = getattr(cur, part)
        _ttype_cache[ttype_str] = cur
        return cur

    def _get_tokens_rust(self, text, encoding=None):
        """Drop-in for Lexer.get_tokens — uses Rust scanner."""
        from io import TextIOBase
        if isinstance(text, TextIOBase):
            text = text.read()
        if isinstance(text, bytes):
            if encoding:
                text = text.decode(encoding)
            else:
                try:
                    text = text.decode('utf-8')
                except UnicodeDecodeError:
                    text = text.decode('unicode-escape')
        elif not isinstance(text, str):
            raise TypeError(
                f"Expected text or file-like object, got {type(text)!r}")
        for ttype_str, value in _rust_tokenize_raw(text):
            yield _resolve(ttype_str), value

    def _tokenize_rust(sql, encoding=None):
        """Module-level tokenize used by FilterStack.run()."""
        return _get_tokens_rust(None, sql, encoding)

    # Patch the Lexer class method
    _lex_mod.Lexer.get_tokens = _get_tokens_rust

    # Patch the module-level function (used directly by filter_stack.py)
    _lex_mod.tokenize = _tokenize_rust

    # filter_stack imports `from sqlparse import lexer` and calls `lexer.tokenize(...)`.
    # Patch that reference too.
    try:
        import sqlparse.engine.filter_stack as _fs
        import sqlparse.lexer  # ensure it's the real module
        _fs.lexer = sqlparse.lexer  # repoint to patched module
    except Exception:
        pass

    # ── parse() patch — Rust grouping ─────────────────────────────────────
    # Replace sqlparse.parse with the Rust implementation that builds the
    # full AST (tokenize → group) in Rust and returns sqlparse.sql objects.

    def _parse_rust(sql: str, encoding=None):
        """Drop-in for sqlparse.parse — Rust tokenize + Rust grouping."""
        return _rust_parse(sql, encoding)

    _sqlparse.parse = _parse_rust

    # ── format() patch — fully Rust ───────────────────────────────────────
    # Replace sqlparse.format with the Rust implementation.

    def _format_rust(sql: str, encoding=None, **options):
        """Drop-in for sqlparse.format — fully in Rust."""
        return _rust_format(sql, encoding, **options)

    _sqlparse.format = _format_rust

    _INSTALLED = True
