"""
sqlparse-rs — drop-in Rust replacement for sqlparse.

This package exposes the same public API as sqlparse 0.6.x:
  parse(), parsestream(), format(), split(), tokenize()

The heavy lifting is done by the Rust extension `sqlparse._rust`.
The Python shim re-exports everything so existing code using
`import sqlparse` continues to work without changes.
"""

from __future__ import annotations

from collections.abc import Generator
from typing import IO, Any

from sqlparse import _rust  # noqa: F401 — triggers Rust module load
from sqlparse._rust import (
    PyToken as _RustToken,
    PyStatement as _RustStatement,
)

# Keep the same __version__ sentinel so version-checking code doesn't break.
__version__ = "0.6.0.rs"

# Re-export sub-modules that users may import directly
from sqlparse import tokens, sql, exceptions  # noqa: F401


def parse(sql_str: str, encoding: str | None = None) -> tuple:
    """Parse sql and return a tuple of Statement instances."""
    return _rust.parse(sql_str, encoding)


def parsestream(stream: str | IO[str], encoding: str | None = None) -> Generator:
    """Lazily parse statements from a string or file-like object."""
    if hasattr(stream, "read"):
        stream = stream.read()
    stmts = _rust.parse(stream, encoding)
    yield from stmts


def format(sql_str: str, encoding: str | None = None, **options: Any) -> str:
    """Format *sql_str* according to *options*.

    Accepts the same keyword arguments as sqlparse.format():
      keyword_case, identifier_case, reindent, strip_whitespace,
      strip_comments, indent_width, indent_tabs, wrap_after,
      comma_first, compact, use_space_around_operators,
      truncate_strings, right_margin, output_format, ...
    """
    return _rust.format(sql_str, encoding, **options)


def split(
    sql_str: str,
    encoding: str | None = None,
    strip_semicolon: bool = False,
) -> list[str]:
    """Split *sql_str* into individual SQL statements."""
    return _rust.split(sql_str, encoding, strip_semicolon)
