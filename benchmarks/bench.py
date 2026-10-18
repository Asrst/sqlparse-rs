#!/usr/bin/env python3
"""Proof 2 — batch benchmarks: Python sqlparse vs sqlparse_rs.

Workloads (all batch; never single queries):
  dump-tokenize    pg_dump-style file through tokenize
  dump-split       same file through split()
  fingerprint      query-log normalization via tokenize + literal stripping
  freeze-profile   Python-only cProfile of format(reindent=True) on a 10k
                   IN-list — quantifies the headroom a Tier 3 port would
                   unlock; NOT a speedup claim (parse/format are not ported).

Method: 1 warmup discarded, min-of-N (default 3), equal-work validation
before any timing is reported (token/statement count + sha256 stream
checksum must match across engines, else the run is VOID).

Output: markdown table (stdout) + results.csv. Sizes tunable via flags.
"""
import argparse
import cProfile
import csv
import gc
import hashlib
import platform
import pstats
import subprocess
import sys
import time

import sqlparse
import sqlparse.lexer
import sqlparse_rs

# bench.py is an A/B comparison harness in a dedicated process. The default
# architecture keeps the engines independent — importing sqlparse_rs has NO
# effect on sqlparse — so every "python" number below is genuinely Python by
# construction. The assert pins that contract (a patch() left installed by
# mistake would silently invalidate the comparison). Rust side is called
# directly via sqlparse_rs.* and is unaffected.
assert not sqlparse_rs.is_patched()


def py_tokens(text):
    return [(str(t), v) for t, v in sqlparse.lexer.tokenize(text)]


def rs_tokens(text):
    return sqlparse_rs.tokenize(text)


def sha_update(h, ttype, value):
    if not isinstance(ttype, str):
        ttype = str(ttype)  # python lexer yields _TokenType objects
    h.update(ttype.encode("utf-8"))
    h.update(b"\x1f")
    h.update(value.encode("utf-8"))
    h.update(b"\x00")


def stream_sha_and_count(tokens):
    """Incremental: hash + count a token iterable without materializing."""
    h = hashlib.sha256()
    n = 0
    for ttype, value in tokens:
        sha_update(h, ttype, value)
        n += 1
    return h.hexdigest(), n


def best_of(fn, runs):
    times = []
    result = None
    for i in range(runs + 1):  # first run = warmup, discarded
        t0 = time.perf_counter()
        result = fn()
        dt = time.perf_counter() - t0
        if i > 0:
            times.append(dt)
    return min(times), result


def make_dump(mb: float) -> str:
    """Deterministic pg_dump-style dump: CREATE TABLE + INSERT batches."""
    target = int(mb * 1024 * 1024)
    parts = ["-- pg_dump-style synthetic dump for benchmarking\n"]
    size = 0
    t = 0
    while size < target:
        cols = ",\n  ".join(
            f"    col{i} {'integer' if i % 3 else 'character varying(64)'}"
            for i in range(8)
        )
        parts.append(
            f"CREATE TABLE public.table_{t} (\n{cols},\n"
            f"    CONSTRAINT pk_{t} PRIMARY KEY (id)\n);\n\n"
        )
        rows = []
        for b in range(0, 40, 10):
            vals = ",\n    ".join(
                "(" + ", ".join(f"'val_{t}_{b}_{i}_{j}', {i * j}" for j in range(6)) + ")"
                for i in range(10)
            )
            rows.append(f"INSERT INTO public.table_{t} VALUES\n{vals};\n")
        chunk = "\n".join(rows) + "\n"
        parts.append(chunk)
        size += len(chunk.encode("utf-8"))
        t += 1
    return "".join(parts)


def make_queries(n: int):
    statuses = ("new", "shipped", "cancelled", "pending", "refunded")
    out = []
    for i in range(n):
        out.append(
            f"SELECT id, total, created_at FROM orders WHERE customer_id = {i} "
            f"AND status = '{statuses[i % 5]}' AND total > {i % 90}.5 "
            f"ORDER BY created_at DESC LIMIT {i % 100};"
        )
    return out


def fingerprint(tokens):
    """Normalize a token stream to a query fingerprint (literals -> L)."""
    parts = []
    for ttype, value in tokens:
        if ttype.startswith("Token.Literal"):
            parts.append("L")
        elif ttype.startswith("Token.Text"):
            continue
        else:
            parts.append(value)
    return "\x00".join(parts)


def machine_specs():
    cpu = ""
    try:
        for line in open("/proc/cpuinfo"):
            if line.startswith("model name"):
                cpu = line.split(":", 1)[1].strip()
                break
    except OSError:
        pass
    try:
        rustc = subprocess.run(
            ["rustc", "--version"], capture_output=True, text=True, timeout=5
        ).stdout.strip()
    except Exception:
        rustc = "n/a"
    return {
        "cpu": cpu or platform.processor() or "unknown",
        "python": platform.python_version(),
        "platform": platform.platform(),
        "sqlparse": sqlparse.__version__,
        "sqlparse_rs": getattr(sqlparse_rs, "__version__", "?"),
        "rustc": rustc,
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--mb", type=float, default=8.0, help="dump size in MB")
    ap.add_argument("--queries", type=int, default=100_000, help="query-log size")
    ap.add_argument("--runs", type=int, default=3, help="timed runs (min-of)")
    args = ap.parse_args()

    specs = machine_specs()
    rows = []  # workload, engine, unit, value, note

    print(f"== Proof 2 — batch benchmarks (equal-work validated, min-of-{args.runs}) ==\n")

    # ---------------- W1: dump tokenize ----------------
    print(f"generating {args.mb} MB dump ...", flush=True)
    dump = make_dump(args.mb)
    print(f"dump ready: {len(dump)/1e6:.1f} MB", flush=True)

    def count_py():
        n = 0
        for _ in sqlparse.lexer.tokenize(dump):
            n += 1
        return n

    def count_rs():
        return len(sqlparse_rs.tokenize(dump))

    t_py, n_py = best_of(count_py, args.runs)
    t_rs, n_rs = best_of(count_rs, args.runs)
    # untimed validation pass: incremental sha on both streams
    sha_py, m_py = stream_sha_and_count(sqlparse.lexer.tokenize(dump))
    rl = sqlparse_rs.tokenize(dump)
    sha_rs, m_rs = stream_sha_and_count(rl)
    del rl
    ok = n_py == n_rs == m_py == m_rs and sha_py == sha_rs
    tag = "" if ok else "  [VOID: streams differ!]"
    rows.append(("dump tokenize", "python", "s", t_py, f"{len(dump)/1e6:.1f} MB"))
    rows.append(("dump tokenize", "rust", "s", t_rs, f"{len(dump)/1e6:.1f} MB"))
    rows.append(("dump tokenize", "speedup", "x", t_py / t_rs, "equal-work ok" if ok else "VOID"))
    rows.append(("dump tokenize", "python throughput", "MB/s", len(dump) / 1e6 / t_py, ""))
    rows.append(("dump tokenize", "rust throughput", "MB/s", len(dump) / 1e6 / t_rs, ""))
    print(f"dump tokenize  python {t_py:8.3f} s ({len(dump)/1e6/t_py:6.1f} MB/s)   "
          f"rust {t_rs:8.3f} s ({len(dump)/1e6/t_rs:6.1f} MB/s)   "
          f"speedup {t_py/t_rs:6.1f}x   tokens {n_rs:,}{tag}", flush=True)
    gc.collect()

    t_py, py_st = best_of(lambda: sqlparse.split(dump), args.runs)
    t_rs, rs_st = best_of(lambda: sqlparse_rs.split(dump), args.runs)
    ok = len(py_st) == len(rs_st)
    if ok:
        h1, h2 = hashlib.sha256(), hashlib.sha256()
        for s_ in py_st:
            sha_update(h1, "stmt", s_)
        for s_ in rs_st:
            sha_update(h2, "stmt", s_)
        ok = h1.hexdigest() == h2.hexdigest()
    tag = "" if ok else "  [VOID: statements differ!]"
    rows.append(("dump split", "python", "s", t_py, f"{len(py_st)} stmts"))
    rows.append(("dump split", "rust", "s", t_rs, f"{len(rs_st)} stmts"))
    rows.append(("dump split", "speedup", "x", t_py / t_rs, "equal-work ok" if ok else "VOID"))
    print(f"dump split     python {t_py:8.3f} s   rust {t_rs:8.3f} s   "
          f"speedup {t_py/t_rs:6.1f}x   statements {len(rs_st):,}{tag}", flush=True)
    del py_st, rs_st, dump
    gc.collect()

    # ---------------- W3: fingerprinting ----------------
    print(f"generating {args.queries:,} queries ...", flush=True)
    queries = make_queries(args.queries)

    def fp(tokenizer):
        return [fingerprint(tokenizer(q)) for q in queries]

    t_py, py_fp = best_of(lambda: fp(py_tokens), args.runs)
    t_rs, rs_fp = best_of(lambda: fp(rs_tokens), args.runs)
    ok = py_fp == rs_fp
    tag = "" if ok else "  [VOID: fingerprints differ!]"
    rows.append(("query-log fingerprint", "python", "s", t_py, f"{len(queries)} queries"))
    rows.append(("query-log fingerprint", "rust", "s", t_rs, f"{len(queries)} queries"))
    rows.append(("query-log fingerprint", "speedup", "x", t_py / t_rs, "fingerprints equal" if ok else "VOID"))
    print(f"fingerprint    python {t_py:8.3f} s   rust {t_rs:8.3f} s   "
          f"speedup {t_py/t_rs:6.1f}x   queries {len(queries):,}{tag}", flush=True)
    del py_fp, rs_fp, queries
    gc.collect()

    # ---------------- W4: freeze profile (Python-only, honest) ----------------
    # Finding: upstream format() REFUSES a 10k-element IN list outright
    # (grouping raises SQLParseError "Maximum number of tokens exceeded").
    # We report that, then profile a size upstream accepts (4k elements).
    from sqlparse.exceptions import SQLParseError

    freeze = "SELECT * FROM t WHERE id IN (" + ",".join(str(i) for i in range(10_000)) + ");"
    try:
        sqlparse.format(freeze, reindent=True)
        rows.append(("freeze 10k IN format", "python", "s", float("nan"), "completed"))
    except SQLParseError as e:
        print(f"freeze 10k IN  python REFUSES: SQLParseError: {e}", flush=True)
        rows.append(("freeze 10k IN format", "python", "error", 0.0,
                     f"upstream raises SQLParseError ({e})"))
    del freeze

    freeze4k = "SELECT * FROM t WHERE id IN (" + ",".join(str(i) for i in range(4_000)) + ");"
    prof = cProfile.Profile()
    t0 = time.perf_counter()
    prof.enable()
    sqlparse.format(freeze4k, reindent=True)
    prof.disable()
    freeze_s = time.perf_counter() - t0
    tok_cum = 0.0
    stats = pstats.Stats(prof)
    for (_fn, _ln, funcname), (_cc, _nc, _tt, ct, _callers) in stats.stats.items():
        if "tokenize" in funcname or funcname == "get_tokens":
            tok_cum = max(tok_cum, ct)
    share = 100.0 * tok_cum / freeze_s if freeze_s else 0.0
    rows.append(("freeze 4k IN format(reindent)", "python", "s", freeze_s, "single run, cProfile"))
    rows.append(("freeze: tokenize share", "python", "%", share,
                 "headroom a Tier 3 port would unlock"))
    print(f"freeze 4k IN   python {freeze_s:8.3f} s (single run)   "
          f"tokenize share ~{share:.0f}%   "
          f"(Tier 3 not ported — no rust number, by design)")

    # ---------------- output ----------------
    with open("benchmarks/results.csv", "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["workload", "engine", "unit", "value", "note"])
        w.writerows(rows)

    print("\n| workload | python | rust | speedup |")
    print("|----------|--------|------|---------|")
    by = {}
    for wl, eng, unit, val, note in rows:
        by.setdefault(wl, {})[eng] = (val, unit, note)
    for wl, d in by.items():
        p = d.get("python", (float("nan"), "", ""))[0]
        r = d.get("rust", (float("nan"), "", ""))[0]
        sp = d.get("speedup", (float("nan"), "", ""))[0]
        if sp == sp:  # not NaN
            print(f"| {wl} | {p:.3f} s | {r:.3f} s | {sp:.1f}x |")
        else:
            print(f"| {wl} | {p:.3f} s | - | - |")

    print("\nmachine:", specs["cpu"])
    print("python:", specs["python"], "| sqlparse:", specs["sqlparse"],
          "| sqlparse_rs:", specs["sqlparse_rs"])
    print("rustc:", specs["rustc"])
    print("wrote benchmarks/results.csv")
    return 0


if __name__ == "__main__":
    sys.exit(main())
