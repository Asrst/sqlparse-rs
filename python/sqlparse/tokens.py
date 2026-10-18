"""
sqlparse.tokens — token type hierarchy.

This is the canonical sqlparse token type module, re-exported so that
code doing ``from sqlparse import tokens as T; T.Keyword.DML`` works as-is.
We re-use the original sqlparse tokens module wholesale — it's tiny (71 lines)
and has zero runtime cost; only the lexer/parser are replaced by Rust.
"""
# Delegate entirely to the original sqlparse tokens module.
# If original sqlparse is installed, use it.  Otherwise fall back to our
# bundled copy.
try:
    from sqlparse.tokens import *   # noqa: F401, F403
    from sqlparse.tokens import Token, Text, Whitespace, Newline, Error   # noqa: F401
    from sqlparse.tokens import Keyword, Name, Literal, String, Number    # noqa: F401
    from sqlparse.tokens import Punctuation, Operator, Comparison         # noqa: F401
    from sqlparse.tokens import Wildcard, Comment, Assignment, Generic    # noqa: F401
    from sqlparse.tokens import DML, DDL, CTE, Command                    # noqa: F401
except ImportError:
    # Bundled fallback — identical to sqlparse/tokens.py
    class _TokenType(tuple):
        parent = None

        def __contains__(self, item):
            return item is not None and (self is item or item[:len(self)] == self)

        def __getattr__(self, name):
            if name.startswith("__"):
                return super().__getattribute__(name)
            new = _TokenType(self + (name,))
            setattr(self, name, new)
            new.parent = self
            return new

        def __repr__(self):
            return "Token" + ("." if self else "") + ".".join(self)

    Token = _TokenType()
    Text = Token.Text
    Whitespace = Text.Whitespace
    Newline = Whitespace.Newline
    Error = Token.Error
    Other = Token.Other
    Keyword = Token.Keyword
    Name = Token.Name
    Literal = Token.Literal
    String = Literal.String
    Number = Literal.Number
    Punctuation = Token.Punctuation
    Operator = Token.Operator
    Comparison = Operator.Comparison
    Wildcard = Token.Wildcard
    Comment = Token.Comment
    Assignment = Token.Assignment
    Generic = Token.Generic
    Command = Generic.Command
    Token.Token = Token
    Token.String = String
    Token.Number = Number
    DML = Keyword.DML
    DDL = Keyword.DDL
    CTE = Keyword.CTE
