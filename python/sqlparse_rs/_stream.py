# sqlparse_rs._stream
# Native implementation of parsestream() — no sqlparse dependency.
#
# Algorithm:
#   1. Read the full text from a str or file-like object.
#   2. Use the Rust tokenizer to produce (ttype_str, value) pairs.
#   3. Run a StatementSplitter (ported from sqlparse.engine.StatementSplitter)
#      on those pairs to yield per-statement token lists.
#   4. For each statement, call our Rust parse() to produce a grouped AST.
#
# The StatementSplitter here is a minimal port of the original sqlparse one —
# same state machine, same block logic, uses sqlparse_rs.tokens constants.

from __future__ import annotations

from typing import Generator, IO

__all__ = ["parsestream"]

from sqlparse_rs import tokens as T
from sqlparse_rs import sql as sql_mod
from sqlparse_rs.sqlparse_rs import tokenize as _rust_tokenize, parse as _rust_parse


# ── ttype resolution cache ────────────────────────────────────────────────────

_ttype_cache: dict = {}


def _resolve(ttype_str: str):
    t = _ttype_cache.get(ttype_str)
    if t is not None:
        return t
    cur = T.Token
    for part in ttype_str.split(".")[1:]:
        cur = getattr(cur, part)
    _ttype_cache[ttype_str] = cur
    return cur


# ── StatementSplitter ─────────────────────────────────────────────────────────
# Ported from sqlparse.engine.StatementSplitter (BSD-3-Clause).
# Operates on (ttype, value) pairs where ttype is a _TokenType object.

class _StatementSplitter:
    """Splits a stream of (ttype, value) pairs into Statement token-lists."""

    def __init__(self):
        self._reset()

    def _reset(self):
        self._block_stack = []
        self._parenthesis_level = 0
        self._unconfirmed_start = None
        self._is_create = False
        self._seen_begin = False
        self.consume_ws = False
        self.tokens: list[sql_mod.Token] = []
        self.level = 0

    def _handle_nested_block(self, unified: str) -> int | None:
        if unified == "FOR":
            self._unconfirmed_start = "FOR"
            return 0
        if unified == "WHILE":
            self._unconfirmed_start = "WHILE"
            return 0
        if unified in ("LOOP", "DO"):
            if self._unconfirmed_start in ("FOR", "WHILE"):
                self._block_stack.append(self._unconfirmed_start)
                self._unconfirmed_start = None
                return 1
            if unified == "LOOP":
                self._block_stack.append("LOOP")
                return 1
        if unified in ("IF", "CASE"):
            self._block_stack.append(unified)
            return 1
        return None

    def _handle_closing_keyword(self, unified: str) -> int:
        if unified == "END IF":
            if self._block_stack and self._block_stack[-1] == "IF":
                self._block_stack.pop()
                return -1
        elif unified == "END FOR":
            if self._block_stack and self._block_stack[-1] == "FOR":
                self._block_stack.pop()
                return -1
        elif unified == "END WHILE":
            if self._block_stack and self._block_stack[-1] == "WHILE":
                self._block_stack.pop()
                return -1
        elif unified == "END LOOP":
            if self._block_stack and self._block_stack[-1] in ("LOOP", "FOR", "WHILE"):
                self._block_stack.pop()
                return -1
        elif unified == "END CASE":
            if self._block_stack and self._block_stack[-1] == "CASE":
                self._block_stack.pop()
                return -1
        elif unified == "END":
            if self._block_stack:
                self._block_stack.pop()
            return -1
        return 0

    def _change_splitlevel(self, ttype, value: str) -> int:
        if ttype is T.Punctuation and value == ";":
            self._unconfirmed_start = None
            if self._seen_begin:
                self._seen_begin = False
                if self._block_stack and self._block_stack[-1] == "BEGIN":
                    self._block_stack.pop()
                    return -1
            return 0

        if ttype is T.Punctuation and value == "(":
            self._parenthesis_level += 1
            return 1
        if ttype is T.Punctuation and value == ")":
            self._parenthesis_level = max(0, self._parenthesis_level - 1)
            return -1

        if ttype not in T.Keyword:
            return 0

        unified = value.upper()

        if ttype is T.Keyword.DDL and unified.startswith("CREATE"):
            self._is_create = True
            return 0

        if unified == "DECLARE" and self._is_create and not self._block_stack:
            self._block_stack.append("DECLARE")
            return 1

        if unified == "BEGIN":
            self._seen_begin = True
            if self._block_stack and self._block_stack[-1] == "DECLARE":
                self._block_stack.pop()
                self._block_stack.append("BEGIN")
                return 0
            else:
                self._block_stack.append("BEGIN")
                return 1

        if self._seen_begin and ttype in (T.Keyword, T.Name) and unified in (
            "TRANSACTION", "WORK", "TRAN",
            "DISTRIBUTED", "DEFERRED",
            "IMMEDIATE", "EXCLUSIVE",
        ):
            self._seen_begin = False
            if self._block_stack and self._block_stack[-1] == "BEGIN":
                self._block_stack.pop()
                return -1
            return 0

        if "BEGIN" in self._block_stack:
            res = self._handle_nested_block(unified)
            if res is not None:
                return res

        return self._handle_closing_keyword(unified)

    def process(self, stream) -> Generator[sql_mod.Statement, None, None]:
        """Yield Statement objects from (ttype, value) stream."""
        EOS_TTYPE = (T.Whitespace, T.Comment.Single)

        for ttype_str, value in stream:
            ttype = _resolve(ttype_str)

            if self.consume_ws and ttype not in EOS_TTYPE:
                yield sql_mod.Statement(self.tokens)
                self._reset()

            self.level += self._change_splitlevel(ttype, value)
            self.tokens.append(sql_mod.Token(ttype, value))

            if ttype is T.Punctuation and value == ";":
                self._seen_begin = False
                if self.level <= 0 and "BEGIN" not in self._block_stack:
                    self.consume_ws = True
            elif ttype is T.Keyword and value.split()[0] == "GO":
                self.consume_ws = True
            elif ttype not in (T.Whitespace, T.Newline, T.Comment.Single, T.Comment.Multiline) \
                    and not (ttype is T.Keyword and value.upper() == "BEGIN"):
                self._seen_begin = False

        if self.tokens and not all(t.is_whitespace for t in self.tokens):
            yield sql_mod.Statement(self.tokens)


# ── Public API ────────────────────────────────────────────────────────────────

def parsestream(
    stream: "str | IO[str]",
    encoding: "str | None" = None,
) -> "Generator[sql_mod.Statement, None, None]":
    """Parse SQL statements from a string or file-like object.

    This is a native implementation — no sqlparse dependency.

    :param stream: A SQL string or any object with a ``.read()`` method.
    :param encoding: Optional encoding for bytes input (default: utf-8).
    :returns: A generator of :class:`~sqlparse_rs.sql.Statement` instances,
              each already grouped (WHERE clauses, identifiers, etc.).
    """
    from io import TextIOBase, RawIOBase, BufferedIOBase

    if hasattr(stream, "read"):
        text = stream.read()
    else:
        text = stream

    if isinstance(text, (bytes, bytearray)):
        text = text.decode(encoding or "utf-8")

    if not isinstance(text, str):
        raise TypeError(f"Expected str or file-like object, got {type(text)!r}")

    # Tokenize with Rust (returns list of (ttype_str, value) pairs).
    raw_tokens = _rust_tokenize(text)

    # Split into individual statements.
    splitter = _StatementSplitter()
    for flat_stmt in splitter.process(iter(raw_tokens)):
        # Re-parse the statement text through Rust grouping so we get the
        # fully grouped AST (Where, Identifier, Function nodes, etc.)
        stmt_text = str(flat_stmt)
        grouped = _rust_parse(stmt_text)
        if grouped:
            yield grouped[0]
