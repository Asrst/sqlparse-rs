# sqlparse-rs — Plan v2 (deltas that change the outcome)

Your plan is sound: architecture, tiers, honesty rails, Bob evidence, and the
scanner spec all cross-check against the 0.6.0 source I verified (lexer
`get_tokens` loop, `is_keyword` dict order, `find_delimited_spans`, full
`statement_splitter`). This document does NOT restate what you already have
right. It contains: **one blocking inconsistency, four process upgrades, a
concrete harness protocol (with a working bootstrap kit), trap lists turned
into ready-made test cases, and benchmark/story fixes.**

Ground rule throughout: your local source read + `tools/dump_regex_table.py`
output are the ground truth. Where my remote verification and your dump ever
disagree, yours wins — but every fact below was confirmed against the real
0.6.0 files, not from memory.

---

## 0. BLOCKING: fix the clock first

- Header says: **deadline 8:59 AM PT, ~10 h budget starting ~1:00 AM PT** (~8 h of runway).
- Schedule table runs **1:00 AM → 8:59 PM** (20 h of runway) and ends "7:00–8:59 PM | Submission".

One of those is wrong, and every downstream gate depends on it. Decide in the
first 5 minutes, then re-anchor everything to **deadline-relative cut lines**
(immune to the AM/PM question):

| T-minus | Gate (hard, no "almost") | If missed |
|---|---|---|
| T−8 h | **Walking skeleton green**: `SELECT abc FROM t;` byte-identical Rust vs Python via harness | Stop new scanner work; fix the harness. Nothing else matters until this is green. |
| T−6 h | **Tier 1 = 100% parity on corpus v1** (≤20 files incl. adversarial shapes below) | Freeze the demo corpus as the "verified subset" slide; keep fixing in background. |
| T−4 h | **Tier 2: `split()` parity** on corpus + splitter quirk files | Demo tokenize-only. Still a complete story; say so plainly. |
| T−2 h | **BENCH.md numbers locked + video recorded** | Video = screen capture of `make check` + bench run; skip the fancy cut. |
| T−1 h | All 6 submission artifacts staged in the repo | Compress improvements statement to bullets. |
| T−0 | Submitted | — |

Non-negotiable operating rule that makes the cut lines survivable: **one
commit per slice**. Any cut point then leaves a coherent, submittable repo.

If the deadline really is 8:59 AM PT: Tier 3 is cut (it already is, conditionally — make it unconditional), keyword codegen happens in hour 1 (not hour 5), and the video is 90 seconds.

---

## 1. Process upgrade: continuous parity, not a big-bang gate

Your schedule has the **first differential check at hour 7** ("7:00–9:00 AM | Differential testing … MUST be green before moving on"). That is the single riskiest line in the plan. If the harness itself, the golden format, or an early semantic assumption is off, you learn it with 2–4 hours left, with the whole scanner written against an unverified contract.

Replace it with **continuous parity**:

1. Hour 1: harness runs end-to-end (kit below does exactly this).
2. T−8 h: first green on `SELECT abc FROM t;` (walking skeleton covers
   precisely this: whitespace, newlines, word+keyword, `;`, Error path).
3. After **every** scanner slice: `make goldens && make check`. Zero diffs is
   *maintained from hour 2*, never "achieved" at hour 7.
4. Your 7:00 AM block then becomes what it should be: corpus expansion,
   adversarial shapes, fuzz — not first contact with reality.

This also de-risks the "326 dialect edge cases" problem: the corpus grows one
green file at a time, and every commit is a provable state.

---

## 2. The harness protocol (this is the product — exact spec, kit included)

### Format
- **Inputs**: `corpus/inputs.bin` — all `.sql` files from `corpus/sql/`,
  NUL-separated (`\x00` never appears in SQL; newlines survive intact).
- **Goldens**: `corpus/goldens.txt` — line-based, produced by `tools/oracle.py`:

  ```
  == rec 3 bytes 1234 sha256 <hex>
  Token.Keyword.DML     53454C454354
  Token.Text.Whitespace 20
  == end
  ```

  One line per token: `str(ttype) <TAB> uppercase-hex(value bytes)`. Hex
  encoding kills every encoding/whitespace/escaping ambiguity. Record *i* in
  the goldens corresponds to record *i* in `inputs.bin` (order-stable; the
  test asserts count equality and fails loudly if they drift).

- **Oracle-error records** (`== rec i ERROR <ExcName>`, e.g. non-UTF-8 input):
  the Rust test **skips** them and reports the count. Skips are allowed;
  mismatches and panics are not.
- **No-panic property is unconditional**: fuzz inputs where the oracle
  succeeds must produce byte-identical output; inputs where it errors must
  still not panic Rust.

### Failure reporting (built into the kit's `tests/differential.rs`)
On the first divergence it prints: record index, token index, expected
`(ttype, hex)` vs actual, and 80 chars of input context — paste that straight
into Bob for triage (see §7). Summary line: `N compared, M skipped, K
mismatched records`. Any K > 0 fails the test.

### Fuzz loop (30 min to build, run it during every break)
Seeded mutator over the corpus: truncate at random byte, splice two records,
duplicate a range, flip ASCII bytes, inject `'`/`"`/`$$`/`/*`/`\`/combining
marks, unbalance quotes and comments. Feed mutated records through the same
oracle→test path. Panic = P0 bug. Divergence = triage into (a) our bug → fix,
(b) intentional divergence → `DIVERGENCES.md` entry + pinned regression file.

### Makefile targets (in kit)
`make corpus goldens check` — plus `split-goldens` for Tier 2 and
`gen-keywords` for codegen. `make check` is the only gate anyone needs to
remember.

---

## 3. Scanner traps → each one is a ready-made test case

Drop each micro-case into `corpus/sql/`, regen goldens, fix until green.
These come from the verified 0.6.0 source; your dump decides exact patterns.

| # | Trap | Why it bites | Micro-case |
|---|---|---|---|
| 1 | `\s+?` is **lazy** → exactly ONE whitespace char per token | Easy to "helpfully" merge runs | `a  b` (two spaces) → two Whitespace tokens. Verify in 10 s: `python3 -c "import sqlparse; print(sqlparse.parse('a  b'))"` |
| 2 | Newline regex `(\r\n|\r|\n)` sits **above** whitespace | Lone `\r` and CRLF must not become WS+Error | `a\r\nb`, `a\rb`, `a\nb` |
| 3 | Error = the regex loop's `for-else`: **single char**, no batching | Scanner temptation: batch unmatchable runs | `}}}}` → four Error tokens |
| 4 | Unresolvable span opener **falls through** to the regex loop | Unclosed `$$` / `/*` does NOT consume to EOF — `resolve()` returns None and the position is lexed normally | `abc $$ 123` — see what the oracle does with the `$` (placeholder vs Error) |
| 5 | `$` has triple duty | Dollar-quote tag (`_DOLLAR_QUOTE_DELIM`, IGNORECASE\|UNICODE), placeholder `$1` (with `(?<!\w)` guard), and word-continuation char in `\w[$#\w]*` | `$$x$$`, `$tag$x$tag$`, `$tag$ unclosed`, `$1`, `a$b`, `$$` |
| 6 | Opener validity: NOT preceded by `\w`, `"`, or `$`; **every** occurrence is a potential closer (bisect resolution) | Misgating breaks `$1 $$2` and word-adjacent dollars | `x$$y$$z`, `"$"` inside strings |
| 7 | Hint comment `(--\|# )\+` — the hash variant needs a **space**: `# ` | `--+hint` is a hint; `#hint` is plain comment, `# +hint` is hint | `--+ h`, `-- h`, `# + h`, `# h`, `--+h` |
| 8 | Comment-to-EOF: `(\r\n\|\r\|\n\|$)` | Trailing comment without newline must swallow to end of input | `SELECT 1 -- trailing` (no final newline) |
| 9 | Keyword dict priority: COMMON → ORACLE → MYSQL → PLPGSQL → HQL → MSACCESS → SNOWFLAKE → BIGQUERY → KEYWORDS, **first dict containing the word wins** | A word in both COMMON and KEYWORDS resolves to COMMON's type; naive merge flips it | Dump `words` manifest, pick 5 words that exist in 2+ dicts, assert the priority winner |
| 10 | `upper()` is Python `str.upper()` (Unicode-aware: `ß→SS`) | Keywords are ASCII in practice → ASCII fast path is safe, but non-ASCII words must not crash or mangle | `select`, `SELECT`, `seLECT`, `ß`, `中文` (all → Name fallback, no panic) |
| 11 | Python `\w` ≠ Rust `char::is_alphanumeric` (Rust's Alphabetic includes combining marks `Mn`; Python `\w` = categories L* + N* + `_`) | Word-boundary divergences on NFD input | `e` + U+0301 (combining acute): Python splits, naive Rust joins. ASCII fast path + general-category fallback |
| 12 | `[<>=~!]+` (Comparison) comes **before** `[+/@#%^&|^-]+` (Operator); JSON ops row sits between; order = the dump | First-match-wins table — reordering by "longest match" silently changes types | `a<>b`, `a!=b`, `a->b`, `a->>b`, `a?|b`, `a#-b`, `@>`, `<@` |
| 13 | Numbers + their lookarounds (`(?=\s*\.(?!\d))` name-with-dot, float `(?![_A-Z])` guards, bracket guard `(?<![\w\])])`) | This is where lookarounds concentrate | `1.`, `.5`, `0x1F`, `1.e3`, `a.b`, `a .b`, `1..2`, `[t1].col`, `1_000` |
| 14 | Strings: `''` doubling (and `\'' per your dump); unclosed string runs to EOF (regex `[^']*$` semantics); backtick/´ with doubling | Scanner must replicate regex-on-EOF, not "error" | `'a''b'`, `'unclosed`, `"sym""x"`, `` `a``b` `` |
| 15 | The dead pattern `(?<=\.)` (upstream FIXME) never matches in Python | Replicate by NOT matching — and put it in the deck | Whatever shape it was meant to catch — prove Python ignores it, prove Rust ignores it identically |
| 16 | Multi-word keywords (`END IF`, `ORDER BY`, `NOT NULL`, `CREATE OR REPLACE`, …) are single tokens — including the whitespace inside them | Which `\s` class and how many spaces? The table decides; don't assume | `END   IF` (three spaces) vs the dump's exact pattern |
| 17 | Placeholders: `?`, `%s`, `%(name)s`, and the `[$:?]\w+` row with its `(?<!\w)` guard | Guard direction (prev char) is easy to flip | `?`, `%s`, `%(x)s`, `:name`, `@name`, `$1`, `a:name` (a-word then colon) |

Suggested slice order (traffic-first, parity after each):
**S1** ws/newline/Error+word+`;` (skeleton) → **S2** comments (single+hint) →
**S3** keyword codegen + unicode word fallback → **S4** numbers → **S5**
strings/quoted idents → **S6** placeholders → **S7** operators/punct/JSON ops →
**S8** delimited spans (dollar quotes, `/* */`, hint variant) → **S9** leftovers
from the dump (`:=`, `::`, `*`, backtick/´, brackets). S8 is last on purpose:
by then everything a span can *contain* is already correct.

## 4. Tier 2 splitter — verified quirk list (each bullet = one test case)

The 0.6.0 `statement_splitter` is a clean 1:1 port (no regex), but its
behavior is full of deliberate weirdness. State to port: `_block_stack`,
`_parenthesis_level`, `_unconfirmed_start`, `_is_create`, `_seen_begin`,
`consume_ws`, `level`, `tokens`.

1. `;` splits only if `level <= 0` **and** `'BEGIN' not in _block_stack` —
   semicolons inside `BEGIN…END` don't split; standalone `BEGIN;` does (it
   pops BEGIN). Test: `BEGIN INSERT 1; INSERT 2; END;` vs `BEGIN; SELECT 1;`.
2. `_seen_begin` + transaction keyword (`TRANSACTION`, `WORK`, `TRAN`,
   `DISTRIBUTED`, `DEFERRED`, `IMMEDIATE`, `EXCLUSIVE` — also when ttype is
   `Name`!) undoes the BEGIN (upstream issue 826). Test: `BEGIN TRANSACTION;`
   must be ONE statement.
3. `DECLARE` opens a block only after `CREATE` (`_is_create` set by
   `Keyword.DDL` starting with CREATE), then transitions to `BEGIN`. Test:
   `CREATE FUNCTION ... DECLARE ... BEGIN ... END;`.
4. Loop starters: `FOR`/`WHILE` only *stage* (`_unconfirmed_start`); `LOOP`/`DO`
   confirm them; bare `LOOP` pushes directly; `IF`/`CASE` push immediately.
   Closers are matched against the stack **top**: `END IF`, `END FOR`,
   `END WHILE`, `END LOOP`, `END CASE`, bare `END` pops anything. Test:
   nested `IF ... END IF;` inside `BEGIN ... END;`.
5. **EOS_TTYPE = Whitespace + Comment.Single — deliberately NOT Newline and
   NOT Comment.Multiline.** After a split, the splitter waits for a
   non-EOS token to yield the pending statement. Consequences:
   `SELECT 1;\n` yields at the Newline; `SELECT 1; -- x` yields after the
   comment; a trailing multiline comment still counts as content. These
   off-by-ones are the classic splitter port bug — one `.sql` per case.
6. `GO` / `GO 2` split (`Keyword` ttype, `value.split()[0] == 'GO'`). Test:
   `SELECT 1 GO 2 SELECT 2`.
7. Final flush: yields if any tokens exist and they're not all whitespace.
   Test: file ending with `;` + newline + blank line + spaces.
8. `_parenthesis_level` floors at 0 (`max(0, …)`) — unbalanced `)` never
   goes negative. Test: `SELECT 1));`.

`make split-goldens` + oracle `split` mode covers all of these; keep the
split goldens in a separate file so lexer parity and splitter parity fail
independently.

---

## 5. Benchmarks that survive judge scrutiny

- **Equal-work protocol**: both the Python oracle and the Rust bench print
  token count + FNV1a-64 of the token stream to stderr; assert the numbers
  match before quoting any speedup. This is your one-line defense against
  "you benchmarked less work."
- hyperfine, three sizes (1 KB / 1 MB / 50 MB), report MB/s and tok/s with
  p50 + p999, machine note in `BENCH.md`. **Deck numbers come only from
  BENCH.md** — no exceptions, including the demo.
- One separate row: **adversarial Error-path input** (e.g. 10 MB of `,,`).
  Python pays the full table walk at every position with zero matches; that
  is the honest worst case and exactly the shape the CVE patches were buying
  down. Frame it as "we deleted the shape," not "we beat Python at its
  worst" — same number, better story.
- CVE-shape linear-scaling plot (yours, keep it) + one superlinear Python
  plot for contrast.
- Fair-fight footnote for the deck: 0.6.0 already has the
  `find_delimited_spans` prepass; you're comparing like-for-like against
  their *best* release. That's a strength — say it.

---

## 6. Story corrections (defensibility under 30% innovation judging)

1. **Use the real regex count from your dump, not "~50".** The 0.6.0
   per-position table is ~16–20 rows plus ~5 prepass regexes over the full
   text. Judges who peek at the source will check. The claim survives at the
   true number and gets *stronger*: "every position pays an ordered walk of
   N backtracking regex calls even where they structurally cannot match."
2. Two depth-evidence gems you already have — surface them on a slide:
   the **dead `(?<=\.)` pattern** (upstream FIXME admits it never matches)
   and **first-match-wins ordering** (the table can't even be reordered for
   performance without changing output — we proved output identical
   *because* order is encoded in the scanner).
3. Keep every honesty guardrail you wrote, verbatim, plus one addition:
   don't claim "lossless" as a property — claim "byte-identical on N inputs
   + M fuzz cases" and let the number carry it.
4. `#![forbid(unsafe_code)]` on the core crate + zero-copy tokens
   (`(TType, Range<usize>)`, no allocation in the hot path) — two free
   credibility points for the technical-execution criterion.

---

## 7. Bob usage (30% of score) — artifact per gate, not one dump at the end

| Block | Bob task | Exported artifact (committed same hour) |
|---|---|---|
| Hour 0–1 | Codebase map; harness review | `docs/bob-sessions/00-map.md`, harness design note |
| Skeleton | Walking-skeleton parity debug | first-divergence report + fix rationale |
| Slices | Regex-table → scanner-spec transcription review | annotated `PORT_MATRIX.md` |
| Codegen | keyword-map generation + manifest diff review | `keywords.rs` + parity test output |
| Corpus | Adversarial shape generation w/ rationale per file | `corpus/adversarial/README.md` |
| Triage | Paste each first-divergence report; commit Bob's fix proposal next to it | `docs/bob-sessions/triage-*.md` |
| Bench | Bench script review + number sanity check | annotated `BENCH.md` |

The pattern judges reward: Bob in the loop at every *gate*, with dated
evidence — which your commit-per-slice rule gives you for free.

---

## 8. Submission buffer rules (from T−2 h)

- Deck skeleton exists by T−6 h (all slides, numbers pending) — filling
  numbers in is 10 minutes, building slides at T−2 h is not.
- Video = screen capture: `make check` green (parity proof), bench side by
  side, CVE scaling plot, one-liner. 90–120 s is enough; do not edit
  footage, do not add music.
- README badges: `forbid(unsafe_code)`, corpus size, parity %, speedup
  table (from BENCH.md).
- License: BSD-3 headers matching upstream + NOTICE noting derivative work
  of python-sqlparse. Judges check provenance; it's 5 minutes now.

---

## 9. Bootstrap kit — works today, 5 commands

Delivered alongside this plan (`bootstrap/`), wired to the protocol above:

| File | Lands at | Does |
|---|---|---|
| `scaffold.sh` | run once | Installs everything below into the repo; **never overwrites** existing files |
| `Cargo.workspace.toml` | `Cargo.toml` | Workspace, release profile (lto, 1 codegen unit) |
| `crates/sqlparse-core/…` | same | Walking skeleton: ws/newline/word+keyword/`;`/Error — compiles std-only, `#![forbid(unsafe_code)]` |
| `crates/sqlparse-core/tests/differential.rs` | same | The parity gate: NUL-record inputs + golden parser, skip-on-oracle-error, first-divergence report, no-panic |
| `Makefile` | repo root | `corpus` `goldens` `check` `gen-keywords` `split-goldens` |
| `tools/oracle.py` | `tools/` | Oracle: `tokenize` / `split` / `words` modes; hex values; tuple-order self-check so it's immune to `(ttype, value)` vs `(value, ttype)` ambiguity |
| `tools/dump_regex_table.py` | `tools/` | Ground-truth `PORT_MATRIX.md`: compiled table w/ flags + actions, span regexes, dict sizes, and `get_tokens` source verbatim |
| `tools/gen_keywords.py` | `tools/` | `keywords.rs` (sorted const array + binary-search lookup, first-dict-wins priority) + manifest for the parity test |
| `tools/build_corpus.py` | `tools/` | NUL-joins `corpus/sql/*.sql` → `inputs.bin` |
| `corpus/sql/00_skeleton_smoke.sql` | `corpus/` | `SELECT abc FROM t;` etc. — the T−8 h first green |

Quickstart on your box:

```bash
bash bootstrap/scaffold.sh /path/to/sqlparse-rs && cd /path/to/sqlparse-rs
python3 -m venv .venv && source .venv/bin/activate && pip install sqlparse==0.6.0
make gen-keywords          # real keyword map from YOUR install + manifest
make corpus goldens check  # first green on the smoke file (expect red on anything else)
# then: slices per §3, goldens+check after each; corpus grows one file at a time
```

Caveats, stated plainly: the kit's Python is standard-library-only and
defensive (attribute fallbacks for 0.6.0 internals) but was **not executed
against a live sqlparse here** (no network in this sandbox) — smoke it on
your box first; any attribute drift is a one-line fix flagged with a clear
message. The Rust side is deliberately minimal and dependency-free so the
first `cargo test` is about your scanner, not about the harness.

---

## 10. What I verified vs. what stays on your machine

Verified against 0.6.0 source this session: `get_tokens` loop semantics
(span-opener precheck → ordered anchored regex walk → single-char Error via
`for-else`), `is_keyword` ordered-dict walk + `upper()`, `set_SQL_REGEX`
compile flags (`IGNORECASE | UNICODE`), `find_delimited_spans` gating
(`'$' in text`, `'/*' in text`) + opener-validity rule + bisect resolution,
and the complete `statement_splitter` state machine (§4 list).

Unverifiable from here (your dump settles it): the exact full `SQL_REGEX`
row set — my two remote reads disagreed (14 vs 15 rows, no word/string/
number rows, one mangled operator row), which is exactly why the kit's
first task is dumping the table locally. Your §4 scanner spec already has
the rows my fetches couldn't see, which tells me your local read is the
good one — treat this document's §3 as the test-case layer on top of it.

