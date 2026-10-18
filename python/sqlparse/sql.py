"""
sqlparse.sql — SQL AST node classes.

Re-exports the PyO3-backed classes from sqlparse._rust, giving them
the same names the original sqlparse.sql module uses.
Users who do ``from sqlparse.sql import Statement, Identifier`` will
get the Rust-backed versions.
"""
from sqlparse._rust import (
    PyToken as Token,
    PyTokenList as TokenList,
    PyStatement as Statement,
    PyIdentifier as Identifier,
    PyIdentifierList as IdentifierList,
    PyParenthesis as Parenthesis,
    PyComparison as Comparison,
    PyWhere as Where,
    PyFunction as Function,
    PyComment as Comment,
)

__all__ = [
    "Token",
    "TokenList",
    "Statement",
    "Identifier",
    "IdentifierList",
    "Parenthesis",
    "Comparison",
    "Where",
    "Function",
    "Comment",
]
