#!/usr/bin/env python3
"""sqlparse-rs differential oracle (tokenize | split | words).

Reads NUL-separated records from stdin (words mode ignores stdin) and writes
a stable, line-based stream that crates/sqlparse-core/tests/differential.rs
parses.

Usage:
  python3 tools/oracle.py tokenize < corpus/inputs.bin > corpus/goldens.txt
  python3 tools/oracle.py split   < corpus/inputs.bin > corpus/goldens-split.txt
  python3 tools/oracle.py words   > corpus/keywords-manifest.txt

Record format:
  == rec <i> bytes <n> sha256 <hex>     record header
  <str(ttype)>\t<UPPERHEX(value)>       one line per token (tokenize mode)
  count <n> + <n> UPPERHEX lines        (split mode)
  == rec <i> ERROR <ExcName>            oracle could not process the record
  == end                                record terminator

Notes:
  * Hex encoding removes every encoding/whitespace/escaping ambiguity.
  * The tuple orientation of tokenize() is auto-detected via a "SELECT 1"
    self-check, so the oracle survives (ttype, value) vs (value, ttype) drift.
  * An oracle ERROR is data (e.g. non-UTF-8 fuzz input), not a crash: the
    Rust side skips such records and reports the count. Mismatches and
    panics are never allowed.
"""
import hashlib
import sys


def emit(line: str = "") -> None:
    sys.stdout.write(line + "\n")


def get_default_lexer():
    from sqlparse.lexer import Lexer

    try:
        return Lexer.get_default_instance()
    except AttributeError:
        lexer = Lexer()
        lexer.default_initialization()
        return lexer


def make_pair_fn(_lexer):
    """fn(text) -> iterator of (ttype, value), via the module-level
    lexer.tokenize (the same entry FilterStack.run uses). Tuple order is
    self-checked against a probe so orientation drift can't bite."""
    import sqlparse.lexer as L

    probe = list(L.tokenize("SELECT 1"))
    values_first = any(p[0] == "SELECT" for p in probe)
    if values_first:
        return lambda text: ((ttype, value) for value, ttype in L.tokenize(text))
    return lambda text: L.tokenize(text)


def run_tokenize(_lexer, text: str) -> None:
    for ttype, value in make_pair_fn(None)(text):
        emit(f"{str(ttype)}\t{value.encode('utf-8').hex().upper()}")


def run_split(text: str) -> None:
    import sqlparse

    statements = sqlparse.split(text)
    emit(f"count {len(statements)}")
    for stmt in statements:
        emit(stmt.encode("utf-8").hex().upper())


def run_words() -> None:
    lexer = get_default_lexer()
    kw_lists = getattr(lexer, "_keywords", None)
    if not kw_lists:
        sys.exit("oracle: lexer has no _keywords attribute; adapt get_default_lexer()")
    emit("# keyword manifest - add_keywords priority order; first dict containing a word wins")
    total = 0
    for i, kwdict in enumerate(kw_lists):
        emit(f"# dict {i} size {len(kwdict)}")
        for word, ttype in kwdict.items():
            emit(f"{word}\t{str(ttype)}")
            total += 1
    emit(f"# total {total}")


def process_records(mode: str) -> None:
    lexer = None
    if mode == "tokenize":
        lexer = get_default_lexer()

    payload = sys.stdin.buffer.read()
    records = payload.split(b"\x00")
    if records and records[-1] == b"":
        records.pop()  # tolerate a trailing NUL

    for i, rec in enumerate(records):
        emit(f"== rec {i} bytes {len(rec)} sha256 {hashlib.sha256(rec).hexdigest()}")
        try:
            text = rec.decode("utf-8")
        except UnicodeDecodeError:
            emit(f"== rec {i} ERROR UnicodeDecodeError")
        else:
            try:
                if mode == "tokenize":
                    run_tokenize(lexer, text)
                else:
                    run_split(text)
            except Exception as exc:  # oracle limitation is data, not a crash
                emit(f"== rec {i} ERROR {type(exc).__name__}")
        emit("== end")


def main() -> None:
    if len(sys.argv) != 2 or sys.argv[1] not in ("tokenize", "split", "words"):
        sys.exit(__doc__)
    mode = sys.argv[1]

    if mode == "words":
        run_words()
        return

    try:
        import sqlparse  # noqa: F401
    except ImportError:
        sys.exit("oracle: pip install sqlparse==0.6.0 first")

    process_records(mode)


if __name__ == "__main__":
    main()
