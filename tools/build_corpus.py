#!/usr/bin/env python3
"""Join corpus/sql/*.sql (sorted by name) into NUL-separated corpus/inputs.bin.

Records must never contain NUL; any found in a source file are replaced with
a space so record boundaries stay unambiguous. Record order is stable, which
the differential test relies on (record i in goldens == record i in inputs).
"""
import pathlib
import sys


def main() -> None:
    root = pathlib.Path(__file__).resolve().parent.parent
    sql_dir = root / "corpus" / "sql"
    files = sorted(sql_dir.glob("*.sql"))
    if not files:
        sys.exit(f"build_corpus: no .sql files under {sql_dir}")
    blobs = [f.read_bytes().replace(b"\x00", b" ") for f in files]
    data = b"\x00".join(blobs)
    out = root / "corpus" / "inputs.bin"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_bytes(data)
    print(f"build_corpus: {len(files)} files, {len(data)} bytes -> {out}")


if __name__ == "__main__":
    main()
