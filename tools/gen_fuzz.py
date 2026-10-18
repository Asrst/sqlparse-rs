#!/usr/bin/env python3
"""Proof 1: seeded fuzz-corpus generator + zero-diff checker.

Assembles N statements from SQL fragments (keywords, literals, operators,
punctuation, placeholders, comments, adversarial truncations) with a seeded
RNG so every diff is reproducible. Deterministic by construction: same seed
-> byte-identical corpus.

Modes:
  gen    write corpus/fuzz/fuzz_<seed>.jsonl (one JSON statement per line)
  check  generate in memory, compare sqlparse vs sqlparse_rs live, report
         first divergences (for minimization into corpus/sql/ regression files)

Bob integration: regenerate with `--seed` / `--count` for fresh live-demo
rounds; the gate must stay at zero.
"""
import argparse
import json
import random
import sys
from pathlib import Path

KEYWORDS = [
    "SELECT", "FROM", "WHERE", "INSERT", "INTO", "VALUES", "UPDATE", "SET",
    "DELETE", "CREATE", "TABLE", "BEGIN", "END", "CASE", "WHEN", "THEN",
    "ELSE", "JOIN", "LEFT", "ON", "GROUP", "BY", "ORDER", "LIMIT", "UNION",
    "ALL", "DISTINCT", "AS", "AND", "OR", "NOT", "NULL", "IN", "BETWEEN",
    "LIKE", "EXISTS", "WITH", "RETURNING", "ILIKE", "AT", "TIME", "ZONE",
    "IF", "LOOP", "GO", "COMMIT", "ROLLBACK", "TRANSACTION",
]
NAMES = ["t", "col", "x", "t1", '"sym"', '"a""b"', "`bt`", "[br]", "a$b", "a#b"]
LITERALS = [
    "1", "2.5", ".5", "1.", "1e5", "1.5x", "0x1F", "'str'", "'it''s'",
    "'a\\'b'", "$$dq$$", "$tag$x$tag$", "'unclosed", "$$unclosed",
]
OPERATORS = [
    "=", "<>", "<=", ">=", "+", "||", "->", "->>", "::", ":=", "@>", "?|",
    "(", ")", "[", "]", ",", ".", ";", "%", "??", "#>>",
]
PLACEHOLDERS = ["?", "%s", "%(n)s", ":name", "$1", "@name"]
COMMENTS = ["-- c\n", "/* b */", "/*+ hint */", "# + h\n", "--+ h\n"]
WHITESPACE = [" ", "\n", "\t", "\r\n", "\r", "\x0c"]

WEIGHTED = (
    [(f, 10) for f in KEYWORDS]
    + [(f, 6) for f in NAMES]
    + [(f, 8) for f in LITERALS]
    + [(f, 6) for f in OPERATORS]
    + [(f, 4) for f in PLACEHOLDERS]
    + [(f, 6) for f in COMMENTS]
    + [(f, 8) for f in WHITESPACE]
)
FRAGMENTS = [f for f, _ in WEIGHTED]
WEIGHTS = [w for _, w in WEIGHTED]
assert len(WEIGHTS) == len(FRAGMENTS)


def make_statement(rng: random.Random) -> str:
    n = rng.randint(1, 40)
    return "".join(rng.choices(FRAGMENTS, weights=WEIGHTS, k=n))


def py_tokens(sqlparse, text):
    import sqlparse.lexer
    return [(str(t), v) for t, v in sqlparse.lexer.tokenize(text)]


def check(count: int, seed: int, verbose: bool) -> int:
    try:
        import sqlparse
        import sqlparse_rs
    except ImportError as e:
        print(f"check needs both sqlparse and sqlparse_rs importable: {e}", file=sys.stderr)
        return 2
    rng = random.Random(seed)
    diffs = 0
    for i in range(count):
        stmt = make_statement(rng)
        expected = py_tokens(sqlparse, stmt)
        actual = sqlparse_rs.tokenize(stmt)
        if expected != actual:
            diffs += 1
            if verbose and diffs <= 5:
                k = next(j for j, (e, a) in enumerate(zip(expected, actual)) if e != a)
                print(f"DIFF at stmt {i} tok {k}: expected {expected[k]!r} "
                      f"got {actual[k]!r}\n  stmt: {stmt!r}\n"
                      f"  ctx expected: {expected[max(0, k-3):k+3]!r}\n"
                      f"  ctx actual:   {actual[max(0, k-3):k+3]!r}")
        elif "".join(v for _, v in actual) != stmt:
            diffs += 1
            print(f"NOT LOSSLESS at stmt {i}: {stmt!r}")
    print(f"seed={seed} count={count} diffs={diffs}")
    return 1 if diffs else 0


def gen(count: int, seed: int, outdir: str) -> int:
    rng = random.Random(seed)
    out = Path(outdir)
    out.mkdir(parents=True, exist_ok=True)
    path = out / f"fuzz_{seed}.jsonl"
    with path.open("w", encoding="utf-8") as f:
        for _ in range(count):
            f.write(json.dumps(make_statement(rng)) + "\n")
    print(f"{path}: {count} statements")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("mode", choices=["gen", "check"])
    ap.add_argument("--count", type=int, default=2000)
    ap.add_argument("--seed", type=int, default=1337)
    ap.add_argument("--outdir", default="corpus/fuzz")
    ap.add_argument("--quiet", action="store_true")
    args = ap.parse_args()
    if args.mode == "gen":
        return gen(args.count, args.seed, args.outdir)
    return check(args.count, args.seed, not args.quiet)


if __name__ == "__main__":
    sys.exit(main())
