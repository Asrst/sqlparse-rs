# sqlparse_rs — mixed Rust/Python package
# The compiled extension lives at sqlparse_rs.sqlparse_rs (the .so inside this package).
# This __init__.py re-exports tokenize/split/parse/format from the extension and
# installs the lexer patch so that sqlparse's parse()/format()/parsestream() also
# use Rust.

from sqlparse_rs.sqlparse_rs import (  # noqa: F401
    tokenize,
    split,
    parse,
    format,
    __version__,
)
from sqlparse_rs._patch import install as _install_patch

# Patch sqlparse's lexer automatically when sqlparse_rs is imported.
_install_patch()
