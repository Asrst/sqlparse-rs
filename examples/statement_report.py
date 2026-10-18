#!/usr/bin/env python3
"""Proof 4 — downstream script: a consumer that only uses the ported surface.

Unmodified downstream code: the SAME script runs against either engine by
pointing DOWNSTREAM_ENGINE at it — no source changes, only the import
environment (exactly how a drop-in accelerator ships).

Reads SQL from files given on argv (or corpus/inputs.bin fallback), then:
  - split() into statements
  - tokenize() each statement, normalize literals -> fingerprint
  - print a report: statement count, per-statement token/fingerprint digest

Run twice and diff:
  DOWNSTREAM_ENGINE=sqlparse    python examples/statement_report.py < inputs > a
  DOWNSTREAM_ENGINE=sqlparse_rs python examples/statement_report.py < inputs > b
  diff a b   # must be empty for the drop-in claim
"""
import hashlib
import os
import sys

if os.environ.get("DOWNSTREAM_ENGINE", "sqlparse") == "sqlparse_rs":
    import sqlparse_rs as sqlparse

    def _tokenize(sql):
        return sqlparse.tokenize(sql)
else:
    import sqlparse
    import sqlparse.lexer

    def _tokenize(sql):
        return sqlparse.lexer.tokenize(sql)


def norm_ttype(t):
    return t if isinstance(t, str) else str(t)


def fingerprint(sql):
    parts = []
    for t, v in _tokenize(sql):
        tt = norm_ttype(t)
        if tt.startswith("Token.Literal"):
            parts.append("L")
        elif not tt.startswith("Token.Text"):
            parts.append(v)
    return hashlib.sha256("\x00".join(parts).encode("utf-8")).hexdigest()[:16]


def main() -> int:
    if len(sys.argv) > 1:
        with open(sys.argv[1], encoding="utf-8", errors="replace") as f:
            data = f.read()
    else:
        data = sys.stdin.buffer.read().decode("utf-8", errors="replace")
    stmts = sqlparse.split(data)
    print(f"engine={os.environ.get('DOWNSTREAM_ENGINE', 'sqlparse')}")
    print(f"statements={len(stmts)}")
    for i, stmt in enumerate(stmts):
        toks = list(_tokenize(stmt))
        ntok = len(toks)
        fp = fingerprint(stmt)
        ws = sum(1 for t, _ in toks if norm_ttype(t).startswith("Token.Text"))
        print(f"stmt {i:05d} tokens={ntok:06d} nonws={ntok - ws:06d} fp={fp}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
