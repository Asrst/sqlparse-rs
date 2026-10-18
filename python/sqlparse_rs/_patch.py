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

    # ── Lexer patch ───────────────────────────────────────────────────────

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

    # Runtime keyword overrides added via Lexer.add_keywords().
    # Maps uppercased word → ttype string (e.g. "MYWORD" → "Token.Keyword").
    # Consulted by _get_tokens_rust after Rust returns Token.Name for an unknown word.
    # Note: parse() and format() call Rust tokenize directly (bypassing this table).
    # add_keywords() only affects paths that go through Lexer.get_tokens / lexer.tokenize,
    # i.e. parsestream() and any code using sqlparse's FilterStack engine.
    _extra_keywords: dict = {}

    def _get_tokens_rust(self, text, encoding=None):
        """Drop-in for Lexer.get_tokens — uses Rust scanner, then applies
        any extra keywords registered via add_keywords()."""
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
            # If Rust returned Token.Name and we have an override for this word,
            # use the override ttype instead.
            if _extra_keywords and ttype_str == 'Token.Name':
                override = _extra_keywords.get(value.upper())
                if override is not None:
                    ttype_str = override
            yield _resolve(ttype_str), value

    def _tokenize_rust(sql, encoding=None):
        """Module-level tokenize used by FilterStack.run()."""
        return _get_tokens_rust(None, sql, encoding)

    # Patch the Lexer class method
    _lex_mod.Lexer.get_tokens = _get_tokens_rust

    # Patch the module-level function (used directly by filter_stack.py)
    _lex_mod.tokenize = _tokenize_rust

    # ── add_keywords() intercept ──────────────────────────────────────────
    # Wrap Lexer.add_keywords so entries are also stored in _extra_keywords,
    # which _get_tokens_rust consults at tokenize time.

    _original_add_keywords = _lex_mod.Lexer.add_keywords

    def _add_keywords_patched(self, keywords: dict):
        _original_add_keywords(self, keywords)
        for word, ttype_obj in keywords.items():
            _extra_keywords[word.upper()] = str(ttype_obj)

    _lex_mod.Lexer.add_keywords = _add_keywords_patched

    # filter_stack imports `from sqlparse import lexer` and calls `lexer.tokenize(...)`.
    # Patch that reference too.
    try:
        import sqlparse.engine.filter_stack as _fs
        import sqlparse.lexer  # ensure it's the real module
        _fs.lexer = sqlparse.lexer  # repoint to patched module
    except Exception:
        pass

    # ── parse() patch — Rust grouping ─────────────────────────────────────

    def _parse_rust(sql: str, encoding=None):
        """Drop-in for sqlparse.parse — Rust tokenize + Rust grouping."""
        return _rust_parse(sql, encoding)

    _sqlparse.parse = _parse_rust

    # ── format() patch — fully Rust ───────────────────────────────────────

    def _format_rust(sql: str, encoding=None, **options):
        """Drop-in for sqlparse.format — fully in Rust."""
        return _rust_format(sql, encoding, **options)

    _sqlparse.format = _format_rust

    _INSTALLED = True
