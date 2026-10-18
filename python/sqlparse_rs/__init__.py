# sqlparse_rs — standalone Rust engine for SQL tokenization, splitting,
# grouping and formatting. ZERO runtime dependency on, and ZERO interaction
# with, the pure-Python `sqlparse` package.
#
# Independence contract (this is the default, and it is tested):
#   * `import sqlparse_rs` does NOT import, modify, or patch `sqlparse`.
#   * `import sqlparse` keeps running the genuine pure-Python engine.
#   * The two engines coexist side by side and can be compared directly in
#     the same interpreter — the benchmark / differential tooling does
#     exactly that (sqlparse is the correctness oracle and the baseline).
#
# Opt-in compatibility mode (YOU decide):
#     import sqlparse_rs
#     sqlparse_rs.patch()      # redirect sqlparse's tokenize/parse/format/
#                              # parsestream to the Rust engine
#     ...                      # run foreign code that does `import sqlparse`
#     sqlparse_rs.unpatch()    # restore the original pure-Python engine
#
# `patch()` is a no-op when sqlparse is not installed, and is skipped when
# the SQLPARSE_RS_NO_PATCH environment variable is set (extra kill-switch).

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

# Explicit opt-in compatibility layer — NEVER applied automatically.
from sqlparse_rs._patch import (  # noqa: F401
    patch,
    unpatch,
    is_patched,
    disabled,
)
