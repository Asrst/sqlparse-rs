# sqlparse_rs.sql
# Drop-in for sqlparse.sql — uses sqlparse_rs.tokens instead of sqlparse.tokens.
# No dependency on sqlparse whatsoever.
#
# Derived from python-sqlparse (BSD-3-Clause):
# https://github.com/andialbrecht/sqlparse

"""This module contains classes representing syntactical elements of SQL."""

import re

from sqlparse_rs import tokens as T


# ── Utilities (inline copies from sqlparse.utils) ────────────────────────────

def imt(token, i=None, m=None, t=None):
    """Helper: Instance / Match / TokenType check."""
    if token is None:
        return False
    if i and isinstance(token, i):
        return True
    if m:
        if isinstance(m, list):
            if any(token.match(*pattern) for pattern in m):
                return True
        elif token.match(*m):
            return True
    if t:
        if isinstance(t, list):
            if any(token.ttype in ttype for ttype in t):
                return True
        elif token.ttype in t:
            return True
    return False


def remove_quotes(val):
    """Remove surrounding quotes from a string."""
    if val is None:
        return None
    if len(val) >= 2 and val[0] in ('"', "'", '`') and val[0] == val[-1]:
        val = val[1:-1]
    return val


# ── Token ─────────────────────────────────────────────────────────────────────

class Token:
    """Base class for all SQL tokens.

    Attributes
    ----------
    ttype : _TokenType or None
        Token type from sqlparse_rs.tokens.  ``None`` for group nodes.
    value : str
        Raw text of this token (never modified).
    is_group : bool
        True for TokenList subclasses, False for leaf tokens.
    is_whitespace : bool
    is_newline : bool
    is_keyword : bool
    normalized : str
        ``value.upper()`` for keywords, else ``value``.
    parent : TokenList or None
    """

    __slots__ = (
        "is_group",
        "is_keyword",
        "is_newline",
        "is_whitespace",
        "normalized",
        "parent",
        "ttype",
        "value",
    )

    def __init__(self, ttype, value):
        value = str(value)
        self.value = value
        self.ttype = ttype
        self.parent = None
        self.is_group = False
        self.is_keyword = ttype in T.Keyword
        self.is_whitespace = ttype in T.Whitespace
        self.is_newline = ttype in T.Newline
        self.normalized = value.upper() if self.is_keyword else value

    def __str__(self):
        return self.value

    def __repr__(self):
        cls = self._get_repr_name()
        value = self._get_repr_value()
        q = '"' if value.startswith("'") and value.endswith("'") else "'"
        return "<{cls} {q}{value}{q} at 0x{id:2X}>".format(
            id=id(self), cls=cls, value=value, q=q
        )

    def _get_repr_name(self):
        return str(self.ttype).split(".")[-1]

    def _get_repr_value(self):
        raw = str(self)
        if len(raw) > 7:
            raw = raw[:6] + "..."
        return re.sub(r"\s+", " ", raw)

    def flatten(self):
        """Yield this token (leaf — no children)."""
        yield self

    def match(self, ttype, values, regex=False):
        """Return True if this token's type and value match *ttype*/*values*."""
        type_matched = self.ttype is ttype
        if not type_matched or values is None:
            return type_matched

        if isinstance(values, str):
            values = (values,)

        if regex:
            flag = re.IGNORECASE if self.is_keyword else 0
            patterns = [re.compile(v, flag) for v in values]
            return any(p.search(self.normalized) for p in patterns)

        if self.is_keyword:
            values = [v.upper() for v in values]

        return self.normalized in values

    def within(self, group_cls):
        """Return True if this token is nested inside *group_cls*."""
        parent = self.parent
        while parent:
            if isinstance(parent, group_cls):
                return True
            parent = parent.parent
        return False

    def is_child_of(self, other):
        return self.parent == other

    def has_ancestor(self, other):
        parent = self.parent
        while parent:
            if parent == other:
                return True
            parent = parent.parent
        return False


# ── TokenList ─────────────────────────────────────────────────────────────────

class TokenList(Token):
    """A group of tokens."""

    __slots__ = ("tokens",)

    def __init__(self, tokens=None):
        self.tokens = tokens or []
        for token in self.tokens:
            token.parent = self
        # Build value from children; ttype is None for groups.
        super().__init__(None, "".join(t.value for t in self.tokens))
        self.is_group = True

    def __str__(self):
        return "".join(t.value for t in self.flatten())

    def __iter__(self):
        return iter(self.tokens)

    def __getitem__(self, item):
        return self.tokens[item]

    def _get_repr_name(self):
        return type(self).__name__

    def _pprint_tree(self, max_depth=None, depth=0, f=None, _pre=""):
        token_count = len(self.tokens)
        for idx, token in enumerate(self.tokens):
            cls = token._get_repr_name()
            value = token._get_repr_value()
            last = idx == (token_count - 1)
            pre = "`- " if last else "|- "
            q = '"' if value.startswith("'") and value.endswith("'") else "'"
            print(f"{_pre}{pre}{idx} {cls} {q}{value}{q}", file=f)
            if token.is_group and (max_depth is None or depth < max_depth):
                parent_pre = "   " if last else "|  "
                token._pprint_tree(max_depth, depth + 1, f, _pre + parent_pre)

    def get_token_at_offset(self, offset):
        idx = 0
        for token in self.flatten():
            end = idx + len(token.value)
            if idx <= offset < end:
                return token
            idx = end

    def flatten(self):
        """Generator yielding leaf tokens recursively."""
        for token in self.tokens:
            if token.is_group:
                yield from token.flatten()
            else:
                yield token

    def get_sublists(self):
        for token in self.tokens:
            if token.is_group:
                yield token

    @property
    def _groupable_tokens(self):
        return self.tokens

    def _token_matching(self, funcs, start=0, end=None, reverse=False):
        if start is None:
            return None, None
        if not isinstance(funcs, (list, tuple)):
            funcs = (funcs,)
        if reverse:
            indexes = range(start - 2, -1, -1)
        else:
            if end is None:
                end = len(self.tokens)
            indexes = range(start, end)
        for idx in indexes:
            token = self.tokens[idx]
            for func in funcs:
                if func(token):
                    return idx, token
        return None, None

    def token_first(self, skip_ws=True, skip_cm=False):
        def matcher(tk):
            return not (
                (skip_ws and tk.is_whitespace)
                or (skip_cm and imt(tk, t=T.Comment, i=Comment))
            )
        return self._token_matching(matcher)[1]

    def token_next_by(self, i=None, m=None, t=None, idx=-1, end=None):
        idx += 1
        return self._token_matching(lambda tk: imt(tk, i, m, t), idx, end)

    def token_not_matching(self, funcs, idx):
        funcs = (funcs,) if not isinstance(funcs, (list, tuple)) else funcs
        funcs = [lambda tk, func=func: not func(tk) for func in funcs]
        return self._token_matching(funcs, idx)

    def token_matching(self, funcs, idx):
        return self._token_matching(funcs, idx)[1]

    def token_prev(self, idx, skip_ws=True, skip_cm=False):
        return self.token_next(idx, skip_ws, skip_cm, _reverse=True)

    def token_next(self, idx, skip_ws=True, skip_cm=False, _reverse=False):
        if idx is None:
            return None, None
        idx += 1

        def matcher(tk):
            return not (
                (skip_ws and tk.is_whitespace)
                or (skip_cm and imt(tk, t=T.Comment, i=Comment))
            )
        return self._token_matching(matcher, idx, reverse=_reverse)

    def token_index(self, token, start=0):
        start = start if isinstance(start, int) else self.token_index(start)
        return start + self.tokens[start:].index(token)

    def group_tokens(self, grp_cls, start, end, include_end=True, extend=False):
        start_idx = start
        start_tok = self.tokens[start_idx]
        end_idx = end + include_end

        if extend and isinstance(start_tok, grp_cls):
            subtokens = self.tokens[start_idx + 1 : end_idx]
            grp = start_tok
            grp.tokens.extend(subtokens)
            del self.tokens[start_idx + 1 : end_idx]
        else:
            subtokens = self.tokens[start_idx:end_idx]
            grp = grp_cls(subtokens)
            self.tokens[start_idx:end_idx] = [grp]
            grp.parent = self

        for token in subtokens:
            token.parent = grp
        return grp

    def insert_before(self, where, token):
        if not isinstance(where, int):
            where = self.token_index(where)
        token.parent = self
        self.tokens.insert(where, token)

    def insert_after(self, where, token, skip_ws=True):
        if not isinstance(where, int):
            where = self.token_index(where)
        nidx, next_ = self.token_next(where, skip_ws=skip_ws)
        token.parent = self
        if next_ is None:
            self.tokens.append(token)
        else:
            self.tokens.insert(nidx, token)

    def has_alias(self):
        return self.get_alias() is not None

    def get_alias(self):
        return None

    def get_name(self):
        return self.get_alias() or self.get_real_name()

    def get_real_name(self):
        return None

    def get_parent_name(self):
        dot_idx, _ = self.token_next_by(m=(T.Punctuation, "."))
        _, prev_ = self.token_prev(dot_idx)
        return remove_quotes(prev_.value) if prev_ is not None else None

    def _get_first_name(self, idx=None, reverse=False, keywords=False, real_name=False):
        tokens = self.tokens[idx:] if idx else self.tokens
        tokens = reversed(tokens) if reverse else tokens
        types = [T.Name, T.Wildcard, T.String.Symbol]
        if keywords:
            types.append(T.Keyword)
        for token in tokens:
            if token.ttype in types:
                return remove_quotes(token.value)
            elif isinstance(token, (Identifier, Function)):
                return token.get_real_name() if real_name else token.get_name()


# ── NameAliasMixin ────────────────────────────────────────────────────────────

class NameAliasMixin:
    """Implements get_real_name and get_alias for Identifier/Function."""

    def get_real_name(self):
        dot_idx = None
        for idx, tok in enumerate(self.tokens):
            if tok.match(T.Punctuation, "."):
                dot_idx = idx
        return self._get_first_name(dot_idx, real_name=True)

    def get_alias(self):
        kw_idx, kw = self.token_next_by(m=(T.Keyword, "AS"))
        if kw is not None:
            return self._get_first_name(kw_idx + 1, keywords=True)
        _, ws = self.token_next_by(t=T.Whitespace)
        if len(self.tokens) > 2 and ws is not None:
            return self._get_first_name(reverse=True)


# ── Concrete node types ───────────────────────────────────────────────────────

class Statement(TokenList):
    """Represents a SQL statement."""

    def get_type(self):
        token = self.token_first(skip_cm=True)
        if token is None:
            return "UNKNOWN"
        if token.ttype in (T.Keyword.DML, T.Keyword.DDL):
            return token.normalized
        if token.ttype == T.Keyword.CTE:
            tidx = self.token_index(token)
            while tidx is not None:
                tidx, token = self.token_next(tidx, skip_ws=True)
                if isinstance(token, (Identifier, IdentifierList)):
                    tidx, token = self.token_next(tidx, skip_ws=True)
                    if token is not None and token.ttype == T.Keyword.DML:
                        return token.normalized
        return "UNKNOWN"


class Identifier(NameAliasMixin, TokenList):
    """Represents an identifier."""

    def is_wildcard(self):
        _, token = self.token_next_by(t=T.Wildcard)
        return token is not None

    def get_typecast(self):
        midx, _ = self.token_next_by(m=(T.Punctuation, "::"))
        _, next_ = self.token_next(midx, skip_ws=False)
        return next_.value if next_ else None

    def get_ordering(self):
        _, ordering = self.token_next_by(t=T.Keyword.Order)
        return ordering.normalized if ordering else None

    def get_array_indices(self):
        for token in self.tokens:
            if isinstance(token, SquareBrackets):
                yield token.tokens[1:-1]


class IdentifierList(TokenList):
    """A list of Identifiers."""

    def get_identifiers(self):
        for token in self.tokens:
            if not (token.is_whitespace or token.match(T.Punctuation, ",")):
                yield token


class TypedLiteral(TokenList):
    """A typed literal like ``date '2001-09-28'``."""
    M_OPEN = [(T.Name.Builtin, None), (T.Keyword, "TIMESTAMP")]
    M_CLOSE = T.String.Single, None
    M_EXTEND = T.Keyword, ("DAY", "HOUR", "MINUTE", "MONTH", "SECOND", "YEAR")


class Parenthesis(TokenList):
    """Tokens between parenthesis."""
    M_OPEN = T.Punctuation, "("
    M_CLOSE = T.Punctuation, ")"

    @property
    def _groupable_tokens(self):
        return self.tokens[1:-1]


class SquareBrackets(TokenList):
    """Tokens between square brackets."""
    M_OPEN = T.Punctuation, "["
    M_CLOSE = T.Punctuation, "]"

    @property
    def _groupable_tokens(self):
        return self.tokens[1:-1]


class Assignment(TokenList):
    """An assignment like ``var := val;``."""


class If(TokenList):
    """An IF clause."""
    M_OPEN = T.Keyword, "IF"
    M_CLOSE = T.Keyword, "END IF"


class For(TokenList):
    """A FOR loop."""
    M_OPEN = T.Keyword, ("FOR", "FOREACH")
    M_CLOSE = T.Keyword, "END LOOP"


class Comparison(TokenList):
    """A comparison expression."""

    @property
    def left(self):
        return self.tokens[0]

    @property
    def right(self):
        return self.tokens[-1]


class Comment(TokenList):
    """A comment."""

    def is_multiline(self):
        return bool(self.tokens) and self.tokens[0].ttype == T.Comment.Multiline


class Where(TokenList):
    """A WHERE clause."""
    M_OPEN = T.Keyword, "WHERE"
    M_CLOSE = T.Keyword, (
        "ORDER BY", "GROUP BY", "LIMIT", "UNION", "UNION ALL",
        "EXCEPT", "INTERSECT", "HAVING", "RETURNING", "INTO",
    )


class Over(TokenList):
    """An OVER clause."""
    M_OPEN = T.Keyword, "OVER"


class Having(TokenList):
    """A HAVING clause."""
    M_OPEN = T.Keyword, "HAVING"
    M_CLOSE = T.Keyword, ("ORDER BY", "LIMIT")


class Case(TokenList):
    """A CASE…END expression."""
    M_OPEN = T.Keyword, "CASE"
    M_CLOSE = T.Keyword, "END"

    def get_cases(self, skip_ws=False):
        CONDITION, VALUE = 1, 2
        ret = []
        mode = CONDITION
        for token in self.tokens:
            if token.match(T.Keyword, "CASE") or (skip_ws and token.ttype in T.Whitespace):
                continue
            elif token.match(T.Keyword, "WHEN"):
                ret.append(([], []))
                mode = CONDITION
            elif token.match(T.Keyword, "THEN"):
                mode = VALUE
            elif token.match(T.Keyword, "ELSE"):
                ret.append((None, []))
                mode = VALUE
            elif token.match(T.Keyword, "END"):
                mode = None
            if mode and not ret:
                ret.append(([], []))
            if mode == CONDITION:
                ret[-1][0].append(token)
            elif mode == VALUE:
                ret[-1][1].append(token)
        return ret


class Function(NameAliasMixin, TokenList):
    """A function or procedure call."""

    def get_parameters(self):
        parenthesis = self.token_next_by(i=Parenthesis)[1]
        result = []
        for token in parenthesis.tokens:
            if isinstance(token, IdentifierList):
                return token.get_identifiers()
            elif imt(token, i=(Function, Identifier, TypedLiteral), t=T.Literal):
                result.append(token)
        return result

    def get_window(self):
        over_clause = self.token_next_by(i=Over)
        if not over_clause:
            return None
        return over_clause[1].tokens[-1]


class Begin(TokenList):
    """A BEGIN/END block."""
    M_OPEN = T.Keyword, "BEGIN"
    M_CLOSE = T.Keyword, "END"


class Operation(TokenList):
    """Grouping of arithmetic operations."""


class Values(TokenList):
    """Grouping of VALUES."""


class Command(TokenList):
    """Grouping of CLI commands."""
