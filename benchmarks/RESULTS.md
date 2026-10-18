# Proofs 2 & 3 — Benchmark + scaling results (banked)

Equal-work validated (token count + sha256 stream checksum asserted equal
across engines before any timing is reported; a mismatch would VOID the row).
1 warmup discarded, min-of-3, single machine, batch workloads only.

| workload | python sqlparse 0.6.0 | sqlparse_rs | speedup | equal work |
|----------|----------------------|-------------|---------|------------|
| dump tokenize (8.8 MB, 2,792,241 tokens) | 13.012 s (0.7 MB/s) | 2.285 s (3.9 MB/s) | **5.7x** | sha256 streams equal |
| dump split (8.8 MB -> 7,600 stmts) | 17.045 s | 1.952 s | **8.7x** | count + sha256 equal |
| query-log fingerprint (100,000 queries) | 24.969 s | 5.068 s | **4.9x** | fingerprint lists equal |

Corpus bench (`cargo run --release -p sqlparse-core --bin bench`, pure Rust,
no PyO3 boundary): 1.35 MB, 103,800 tokens — Python 0.777 s vs Rust 0.114 s,
**~6.8x**, token count + stream checksum asserted equal.

## Proof 3 — adversarial scaling (banked)

`benchmarks/scaling.py`: unclosed `$$` chains and comment-only input at
1 KB → 10 MB, `tokenize()` wall time, min of 3 runs. Python points are
measured in a subprocess with a **120 s timeout guard**; identical input
bytes go to both engines. Visual: `scaling.png` · raw: `scaling.csv`.

| shape | python sqlparse 0.6.0 | sqlparse_rs | speedup |
|-------|----------------------|-------------|---------|
| unclosed `$$`, 1 MB | 8.014 s | 1.860 s | 4.3x |
| unclosed `$$`, 10 MB | **DNF — not finished in 120 s** | 18.63 s | **≥ 6.4x (lower bound)** |
| comment-only, 10 MB | 0.813 s | 0.271 s | 3.0x |

Honest notes (deck-ready):

1. **What the DNF proves — and what it does not.** Python is linear on these
   shapes at small sizes; the 10 MB DNF is the wall, not a hidden asymptotic
   bug in 0.6.0. The claim is "Python hits a wall here, Rust does not", not
   "Python is non-linear everywhere".
2. **The 10 MB speedup is a lower bound.** Python's true time is unknown
   above the guard, so only 120/18.6 = 6.4x can be asserted ("≥ 6.4x").
3. **Comment-only input is the fairness control**: both curves are linear
   with a constant 3–6x gap — the same constant factor Proof 2 measures on
   benign workloads. No divergence tricks were needed to win there.

## Machine

- Intel(R) Xeon(R) Processor (sandboxed single-tenant VM)
- Python 3.12.15, sqlparse 0.6.0, sqlparse_rs 0.1.0 (maturin develop --release)
- rustc 1.99.1, release profile

## Honest notes (deck-ready)

1. **Why 5-9x and not 10-50x.** These are END-TO-END numbers through the
   Python API: `tokenize` must materialize 2.79M Python tuples + value
   strings, and that construction dominates on token-dense input (the pure
   Rust scanner alone runs at 11.3 MB/s on the corpus bench, ~3x the
   end-to-end rate). `split` returns 7,600 objects instead of 2.79M, which
   is exactly why it shows the bigger number (8.7x). Quote the table, not
   the aspiration. The obvious next lever is a zero-copy iterator protocol
   or a preallocated-array return (future work, not claimed).
2. **The freeze workload is a refusal, not a slowdown.** Upstream
   `format(reindent=True)` on a 10,000-element IN list does not run slowly —
   it RAISES `SQLParseError: Maximum number of tokens exceeded (10000)`.
   At 4,000 elements it formats in 0.914 s and only ~12% of that time is
   tokenization — the bottleneck is grouping. Porting Tier 3 (grouping) is
   the real unlock and is future work; we do not claim this workload.
3. **Fingerprinting is per-query overhead bound.** 100k separate calls pay
   Python-call overhead on both sides; the 4.9x reflects steady-state
   scanner advantage, not batch amortization.

## Reproduce

```bash
python benchmarks/bench.py                 # defaults: 8 MB dump, 100k queries
python benchmarks/bench.py --mb 25 --queries 1000000   # bigger, slower
python benchmarks/scaling.py               # Proof 3 sweep (~10 min: 2x 120 s DNF guards)
python benchmarks/scaling.py --plot-only   # re-render scaling.png from banked CSV
cargo run --release -p sqlparse-core --bin bench < corpus/inputs.bin
```

Raw numbers: `benchmarks/results.csv`.
