#!/usr/bin/env python3
"""Proof 1: generate dialect keyword sweep records into corpus/sql/.

One corpus record per sqlparse keyword dict (first-dict-wins priority order):
every keyword appears in Upper and lower form inside a minimal statement.
Tokenizer doesn't care about grammar validity, so this is safe.

Usage: python tools/gen_dialect_sweep.py [outdir]
"""
import sys
from pathlib import Path

from sqlparse import keywords as K

# Priority order per sqlparse.lexer.is_keyword (first dict containing the
# word wins). Bare KEYWORDS is the fallback table.
DIALECTS = [
    ("common", K.KEYWORDS_COMMON),
    ("oracle", K.KEYWORDS_ORACLE),
    ("mysql", K.KEYWORDS_MYSQL),
    ("plpgsql", K.KEYWORDS_PLPGSQL),
    ("hql", K.KEYWORDS_HQL),
    ("msaccess", K.KEYWORDS_MSACCESS),
    ("snowflake", K.KEYWORDS_SNOWFLAKE),
    ("bigquery", K.KEYWORDS_BIGQUERY),
    ("keywords_bare", K.KEYWORDS),
]


def main(outdir: str) -> int:
    out = Path(outdir)
    out.mkdir(parents=True, exist_ok=True)
    total = 0
    for name, table in DIALECTS:
        words = sorted(w for w in table if w.isascii() and w.isalnum())
        stmts = []
        for w in words:
            stmts.append(f"SELECT {w} FROM t;")
            stmts.append(f"select {w.lower()} from t;")
        path = out / f"30_dialect_{name}.sql"
        path.write_text("\n".join(stmts) + "\n", encoding="utf-8")
        print(f"{path.name}: {len(words)} keywords x2 = {len(stmts)} statements")
        total += len(stmts)
    print(f"total: {total} statements in {len(DIALECTS)} records")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1] if len(sys.argv) > 1 else "corpus/sql"))
