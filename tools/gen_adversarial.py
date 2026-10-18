#!/usr/bin/env python3
"""Proof 1: generate large adversarial records into corpus/sql/.

Deterministic shapes (Proof 1 item 3 of PROOF_PLAN.md):
  - 44_parens_500.sql       500-deep nested parentheses
  - 45_in_list_10k.sql      10,000-element IN list
  - 46_dollar_unclosed_10k.sql  10,000 unclosed $tag$ openers

Usage: python tools/gen_adversarial.py [outdir]
"""
import sys
from pathlib import Path


def main(outdir: str) -> int:
    out = Path(outdir)
    out.mkdir(parents=True, exist_ok=True)

    (out / "44_parens_500.sql").write_text(
        "SELECT " + "(" * 500 + "1" + ")" * 500 + ";\n"
        + "SELECT " + "(" * 250 + "1" + ")" * 250 + " FROM t WHERE x IN "
        + "(" * 100 + "1" + ")" * 100 + ";\n",
        encoding="utf-8",
    )
    print("44_parens_500.sql: 500-deep + mixed nesting")

    (out / "45_in_list_10k.sql").write_text(
        "SELECT * FROM t WHERE id IN (" + ",".join(str(i) for i in range(10_000)) + ");\n"
        + "SELECT * FROM u WHERE name IN (" + ",".join(f"'s{i}'" for i in range(5_000)) + ");\n",
        encoding="utf-8",
    )
    print("45_in_list_10k.sql: 10k int elements + 5k string elements")

    (out / "46_dollar_unclosed_10k.sql").write_text(
        "SELECT " + "$$" * 10_000 + " 1;\n"
        + "SELECT $a$ never closed, $b$ also open;\n",
        encoding="utf-8",
    )
    print("46_dollar_unclosed_10k.sql: 10k unclosed $$ + chained unclosed $tags$")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1] if len(sys.argv) > 1 else "corpus/sql"))
