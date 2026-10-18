# sqlparse_rs.tokens
# Pure-Python _TokenType hierarchy — API-compatible with sqlparse.tokens.
# Fully standalone: this module NEVER imports sqlparse, at import time or
# any other time (pinned by tests/test_independence.py).
#
# _TokenType is a subclass of tuple; each node lazily creates children via
# __getattr__ and stores them as attributes (cached after first access).
# Containment: `child in parent` is True iff child starts with parent's tuple.
#
# The module-level aliases (Token, Keyword, DML, Whitespace, …) match the
# exact names exported by sqlparse.tokens so that code doing:
#
#     from sqlparse_rs.tokens import Keyword, DML, Whitespace
#
# works identically when sqlparse_rs.tokens is used as a drop-in.
#
# _TokenType object IDENTITY with sqlparse.tokens is only (optionally)
# established inside sqlparse_rs.patch() — an explicit, user-driven action.
# By default the two hierarchies are independent (structurally identical,
# equal by value, distinct by identity).


class _TokenType(tuple):
    parent = None

    def __contains__(self, item):
        return item is not None and (self is item or item[: len(self)] == self)

    def __getattr__(self, name):
        # Don't interfere with dunder lookups.
        if name.startswith("__"):
            raise AttributeError(name)
        new = _TokenType(self + (name,))
        setattr(self, name, new)
        new.parent = self
        return new

    def __repr__(self):
        return "Token" + ("." if self else "") + ".".join(self)

    def ttype(self):
        return self

    def subtypes(self):
        return []

    def is_parent(self, other):
        return other is not None and other[: len(self)] == self


# ── Root ──────────────────────────────────────────────────────────────────────

Token = _TokenType()

# ── Aliases (matches sqlparse.tokens public surface exactly) ─────────────────

# Text / Whitespace
Text       = Token.Text
Whitespace = Token.Text.Whitespace
Newline    = Token.Text.Whitespace.Newline

# Error
Error = Token.Error

# Keywords
Keyword    = Token.Keyword
DDL        = Token.Keyword.DDL
DML        = Token.Keyword.DML
CTE        = Token.Keyword.CTE

# Name
Name = Token.Name

# Literals
Literal = Token.Literal
String  = Token.Literal.String
Number  = Token.Literal.Number

# Operators
Operator   = Token.Operator
Comparison = Token.Operator.Comparison

# Other
Punctuation = Token.Punctuation
Wildcard    = Token.Wildcard
Comment     = Token.Comment
Assignment  = Token.Assignment
Other       = Token.Other
Generic     = Token.Generic

__all__ = [
    "_TokenType",
    "Token",
    # Text / Whitespace
    "Text",
    "Whitespace",
    "Newline",
    # Error
    "Error",
    # Keywords
    "Keyword",
    "DDL",
    "DML",
    "CTE",
    # Name
    "Name",
    # Literals
    "Literal",
    "String",
    "Number",
    # Operators
    "Operator",
    "Comparison",
    # Other
    "Punctuation",
    "Wildcard",
    "Comment",
    "Assignment",
    "Other",
    "Generic",
]
