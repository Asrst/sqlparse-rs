# sqlparse_rs._patch — EXPLICIT opt-in compatibility layer.
#
# Calling sqlparse_rs.patch() monkey-patches sqlparse's lexer AND top-level
# parse/format/parsestream to use Rust. Calling unpatch() restores the
# original pure-Python engine exactly (attribute-granular, nothing left
# behind).
#
# There is NO automatic patching anywhere in this package:
#   * `import sqlparse_rs` never touches sqlparse (see tests/test_independence.py).
#   * patch() only runs when YOU call it.
#
# Safety valves:
#   * patch() is a no-op when sqlparse is not installed.
#   * Set SQLPARSE_RS_NO_PATCH=1 to make patch() a no-op (useful for A/B
#     comparison scripts that inherit a patched environment).
#   * `disabled()` is a context manager that temporarily restores the
#     pure-Python engine (used by the differential/benchmark harnesses so
#     the Python side of a Python-vs-Rust comparison stays genuinely
#     Python even while the patch is active in the same interpreter).
#   * `is_patched()` reports whether the patch is currently installed.

import contextlib
import os

__all__ = [
    "is_patched",
    "patch",
    "unpatch",
    "disabled",
    # Backwards-compatible aliases
    "install",
    "uninstall",
]

_INSTALLED = False
_ORIGINALS: dict = {}


def is_patched():
    """True while sqlparse's engine is redirected to Rust."""
    return _INSTALLED


def patch():
    """EXPLICITLY redirect sqlparse's tokenize/parse/format to Rust.

    Idempotent. No-op if sqlparse is not installed or if the
    SQLPARSE_RS_NO_PATCH environment variable is set.

    Also shares sqlparse's _TokenType singletons into sqlparse_rs.tokens
    (one-way, so `ttype is T.Keyword` identity checks hold across engines
    while the patch is active). Sharing is not reverted by unpatch(): the
    two hierarchies are structurally identical and code that already holds
    references must stay valid.
    """
    global _INSTALLED
    if _INSTALLED:
        return
    if os.environ.get("SQLPARSE_RS_NO_PATCH"):
        return

    try:
        import sqlparse.lexer as _lex_mod
        import sqlparse.tokens as _T
        import sqlparse as _sqlparse
    except ImportError:
        # sqlparse not installed — nothing to patch
        return

    # Snapshot the original Python engine so unpatch() can restore it
    # exactly (attribute-granular, nothing left behind).
    _ORIGINALS.clear()
    _ORIGINALS["lexer.get_tokens"] = (_lex_mod.Lexer, "get_tokens",
                                      _lex_mod.Lexer.__dict__.get("get_tokens"))
    _ORIGINALS["lexer.add_keywords"] = (_lex_mod.Lexer, "add_keywords",
                                        _lex_mod.Lexer.__dict__.get("add_keywords"))
    _ORIGINALS["lexer.tokenize"] = (_lex_mod, "tokenize", _lex_mod.__dict__.get("tokenize"))
    _ORIGINALS["sqlparse.parse"] = (_sqlparse, "parse", _sqlparse.__dict__.get("parse"))
    _ORIGINALS["sqlparse.format"] = (_sqlparse, "format", _sqlparse.__dict__.get("format"))
    _ORIGINALS["sqlparse.parsestream"] = (_sqlparse, "parsestream",
                                          _sqlparse.__dict__.get("parsestream"))

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
        _ORIGINALS["filter_stack.lexer"] = (_fs, "lexer", _fs.__dict__.get("lexer"))
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

    # ── parsestream() patch — native, no sqlparse engine ──────────────────

    from sqlparse_rs._stream import parsestream as _native_parsestream

    _sqlparse.parsestream = _native_parsestream

    # ── Token singleton sharing (moved out of tokens.py) ──────────────────
    # Only happens on EXPLICIT patch(): rebind sqlparse_rs.tokens' public
    # aliases to sqlparse's _TokenType objects so identity checks hold
    # across engines. One-way; see patch() docstring.

    import sqlparse_rs.tokens as _rs_tokens

    for _name in dir(_T):
        if not _name.startswith("_"):
            setattr(_rs_tokens, _name, getattr(_T, _name))

    _INSTALLED = True


def unpatch():
    """Restore the original pure-Python sqlparse engine (no-op if not patched)."""
    global _INSTALLED
    if not _INSTALLED:
        return
    for _owner, _name, _orig in _ORIGINALS.values():
        if _orig is not None:
            setattr(_owner, _name, _orig)
        else:
            try:
                delattr(_owner, _name)
            except AttributeError:
                pass
    _ORIGINALS.clear()
    _INSTALLED = False


# Backwards-compatible aliases (the layer used to be install()/uninstall()).
install = patch
uninstall = unpatch


@contextlib.contextmanager
def disabled():
    """Context manager: temporarily restore the pure-Python engine.

    Used by the differential/benchmark harnesses so the Python side of a
    Python-vs-Rust comparison is genuinely Python even when the explicit
    patch is active in the same interpreter. No-op when patch() was never
    called (the default).
    """
    was_installed = _INSTALLED
    unpatch()
    try:
        yield
    finally:
        if was_installed:
            patch()
