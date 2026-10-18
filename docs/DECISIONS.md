# Architecture Decisions

This file records the key design choices made in this project and the reasoning
behind them, grounded in what was actually built and observed in the code.

---

## Decision 1: Hand-written scanner instead of the `regex` crate

**Chosen:** Hand-written character-scanning rules in `crates/sqlparse-core/src/lexer.rs`  
**Rejected:** Compiling Python's 50 `SQL_REGEX` patterns into Rust `Regex` objects (the approach used in the old `src/lexer.rs` scaffold, commit `b96a4bc`)

### What the old approach did

```rust
// b96a4bc: src/lexer.rs
static RULES: Lazy<Vec<Rule>> = Lazy::new(|| {
    keywords::sql_regex_rules()
        .into_iter()
        .map(|(pat, tt)| Rule {
            re: Regex::new(pat).expect("bad regex"),
            ttype: tt,
        })
        .collect()
});

// At every position in the input:
for rule in RULES.iter() {
    if let Some(m) = rule.re.find(slice) { ... }
}
```

At every character position it called `.find()` on up to 50 compiled `Regex`
objects in sequence. Each `.find()` invokes the full NFA/DFA regex engine.

### Why that is still slow

The Rust `regex` crate is fast, but its semantics are not the same as Python's
`re`. Python's `re` uses a backtracking NFA engine. The `regex` crate uses a
DFA with no backtracking. For most of the 50 rules this doesn't matter, but for
the rules that rely on backtracking — number literals with lookaheads/lookbehinds
(`(?![_A-Z])` guards), quoted strings with greedy+unwind semantics, the
`(?<=\.)` lookbehind — you cannot compile the Python pattern into a `regex`
crate `Regex` and get identical output. The old scaffold would have produced
*different* tokens for inputs like `1.5`, `.5`, `'it''s'`, `a.b`, and
`[t1].col`.

Additionally, running 50 regex engines at every position is still O(n × 50 ×
regex_cost). Even though Rust's `regex` crate is faster than Python's `re`, the
structural cost is the same. You wouldn't get byte-identical output, and you
still pay for 50 DFA traversals per character.

### Why the hand-written scanner is better

Each of the 50 rules is encoded as explicit character checks in `rule_at()`. At
a `;` character, rules 0–45 all fail on their first character check (typically a
single `if ch[pos] == X` comparison) and the match lands at rule 46 after ~10
comparisons total — no engine overhead at all.

For the rules that require backtracking behavior, the scanner reproduces the
exact Python `re` semantics explicitly:

- **Quoted strings** (`scan_quoted()`): greedy scan with a stack-based unwind
  that replicates Python regex backtracking when EOF is hit mid-string.
- **Number literals** (`r_float()`, `r_sci()`, `r_int()`): shrink loops that
  try the longest match first and unwind, reproducing `(?![_A-Z])` guard
  semantics.
- **`name_dot()`**: reverse-scan (longest first) with whitespace shrink,
  reproducing the lookahead `(?=\s*\.(?!\d))`.

The result is verified byte-identical against real sqlparse 0.6.0 output on 24
fixture files via the differential golden-file tests in
`crates/sqlparse-core/tests/differential.rs`.

### Is the hand-written scanner faster than using the `regex` crate?

For this specific workload: **yes, for most inputs.** The reasons:

1. **Early exit per rule.** Each rule tests one or two characters before
   deciding it can't match. A whitespace character reaches rule 3 (a single
   `is_py_ws(ch[pos])` test) and returns in ~3 comparisons. The `regex` crate
   engine still has to set up a DFA scan even for a trivially-failing pattern.

2. **No allocation in the hot path.** Token values are zero-copy slices
   (`&'a str`) of the input. The old scaffold did `m.as_str().to_string()` —
   one heap allocation per token.

3. **No regex engine startup cost.** The `regex` crate amortises compilation
   via `Lazy<Vec<Rule>>`, but the DFA traversal still runs per call.

4. **`forbid(unsafe_code)` is free.** Since no `regex` crate, no NFA/DFA state
   machine; the scanner is pure safe Rust with no unsafe blocks.

The tradeoff: the hand-written scanner is harder to modify than a regex table.
Adding a new token rule requires writing a new helper function rather than
adding a pattern string. This is acceptable because the rule set is fixed by the
sqlparse 0.6.0 specification — we are not designing a new language, we are
porting one.

---

## Decision 2: Zero dependencies on `sqlparse-core`

**Chosen:** `sqlparse-core/[dependencies]` is intentionally empty.  
**Rejected:** Using `regex`, `once_cell`, `nom`, `pest`, or any other parsing library.

Rationale: the only consumer of `sqlparse-core` is `sqlparse-py`, which wraps
it in PyO3. Keeping the core dependency-free means:

- The crate compiles with `std` only — no linker surprises.
- No license compatibility issues beyond BSD-3.
- Binary size is minimised (matters for a Python extension module).
- The `regex` crate's DFA tables alone add ~200 KB to a release binary.

PyO3 lives only in `crates/sqlparse-py` and never touches the scanning logic.

---

## Decision 3: Module name `sqlparse_rs`, not `sqlparse._rust`

**Chosen:** `module-name = "sqlparse_rs"` in `crates/sqlparse-py/pyproject.toml`  
**Rejected:** `module-name = "sqlparse._rust"` with a `python-source = "python/"` shim
(the approach in commit `b96a4bc`)

The old scaffold installed itself as a submodule of a package also named
`sqlparse`. When both the real `sqlparse` (0.6.0, installed in the venv) and
the shim `python/sqlparse/` exist, Python's import system picks one based on
`sys.path` ordering — and `maturin develop` puts the maturin-installed package
*before* the pip-installed one, causing a circular import: the shim's
`from sqlparse import _rust` tries to import from itself.

Installing as `sqlparse_rs` (a separate top-level module) avoids this entirely.
The drop-in `import sqlparse` compatibility is a separate layer that can be
added later once `parse()` and `format()` are Rust-backed and the shim can be
trusted to not recurse.

---

## Decision 4: Differential golden-file testing, not unit tests alone

**Chosen:** `corpus/goldens.txt` generated from real sqlparse 0.6.0 as the
primary correctness gate.  
**Rejected:** Writing expected values by hand in unit tests.

The sqlparse 0.6.0 lexer has dozens of quirks that are not documented and not
obvious from the source — lazy whitespace (one char per token, not runs), `GO`
splitting behavior, `BEGIN TRANSACTION` undoing the block stack, `EOS_TTYPE`
excluding `Newline`, etc. Hand-written unit tests would only cover the cases the
author thought to test. Golden files capture the actual behavior of the
reference implementation on a broad corpus and make any divergence immediately
visible, including for cases nobody anticipated.

The golden format uses hex-encoded values (not raw strings) to eliminate any
ambiguity from whitespace, encoding, or line ending differences when comparing
Rust output to Python output.
