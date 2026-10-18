# sqlparse_rs — mixed Rust/Python package
# The compiled extension lives at sqlparse_rs.sqlparse_rs (the .so inside this package).
# This __init__.py re-exports tokenize/split/parse/format/parsestream from the
# extension, provides our own sql/tokens modules (no sqlparse dependency), and
# optionally installs the lexer patch when sqlparse is available.

from sqlparse_rs.sqlparse_rs import (  # noqa: F401
    tokenize,
    split,
    parse,
    format,
    __version__,
)

# Native parsestream() — no sqlparse dependency.
from sqlparse_rs._stream import parsestream  # noqa: F401

# Our own pure-Python sql / tokens modules (no sqlparse dependency).
from sqlparse_rs import sql    # noqa: F401
from sqlparse_rs import tokens  # noqa: F401

# Optionally patch sqlparse's lexer if sqlparse is installed.
try:
    from sqlparse_rs._patch import install as _install_patch
    _install_patch()
except ImportError:
    pass  # sqlparse not installed — patch is a no-op
