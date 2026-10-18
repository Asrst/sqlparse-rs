PY      ?= python3
CORPUS  := corpus
INPUTS  := $(CORPUS)/inputs.bin
GOLDENS := $(CORPUS)/goldens.txt

.PHONY: corpus goldens split-goldens check gen-keywords port-matrix build-py test-py bench-py dist clean

## Rebuild inputs.bin from corpus/sql/*.sql (sorted, NUL-joined)
corpus: $(INPUTS)
$(INPUTS): tools/build_corpus.py $(wildcard $(CORPUS)/sql/*.sql)
	$(PY) tools/build_corpus.py

## Regenerate goldens from the real sqlparse (the referee)
goldens: $(GOLDENS)
$(GOLDENS): $(INPUTS) tools/oracle.py
	$(PY) tools/oracle.py tokenize < $(INPUTS) > $(GOLDENS)

split-goldens: $(INPUTS)
	$(PY) tools/oracle.py split < $(INPUTS) > $(CORPUS)/goldens-split.txt

## THE gate: full parity + unit tests
check: $(GOLDENS)
	cargo test -p sqlparse-core

## Regenerate the keyword map + manifest from the installed sqlparse
gen-keywords:
	$(PY) tools/gen_keywords.py crates/sqlparse-core/src/keywords.rs $(CORPUS)/keywords-manifest.txt

## Regenerate PORT_MATRIX.md (ground-truth rule table)
port-matrix:
	$(PY) tools/dump_regex_table.py > PORT_MATRIX.md

## Build the PyO3 extension into the active venv (mixed package layout:
## run from repo root — root pyproject.toml sets python-source=python)
build-py:
	maturin develop --release

## Python-side tests (wrapper + parity vs sqlparse)
test-py: build-py
	$(PY) -m pytest tests/test_wrapper.py -q

bench-py: $(INPUTS)
	$(PY) tools/oracle.py tokenize < $(INPUTS) > /dev/null

## Build a release wheel for the current platform into dist/
dist:
	maturin build --release --out dist/

clean:
	rm -f $(INPUTS) $(GOLDENS)

## Proof 1: python live-vs-live parity gate (writes tests/parity-report.md)
proof-parity: build-py
	$(PY) -m pytest tests/test_parity.py -q

## Proof 1: regenerate dialect sweep + adversarial corpus records
sweep:
	$(PY) tools/gen_dialect_sweep.py corpus/sql
	$(PY) tools/gen_adversarial.py corpus/sql

## Proof 1: regenerate committed fuzz corpus (deterministic, seed 1337)
fuzz:
	$(PY) tools/gen_fuzz.py gen --count 2000 --seed 1337 --outdir corpus/fuzz
fuzz-check:
	$(PY) tools/gen_fuzz.py check --count 2000 --seed 1337

## Proof 2: batch benchmarks (writes benchmarks/results.csv)
bench: build-py
	$(PY) benchmarks/bench.py

## Proof 3: adversarial scaling plot (writes benchmarks/scaling.png)
scaling: build-py
	$(PY) benchmarks/scaling.py

## Proof 4: downstream integration (writes tests/downstream-report.md)
downstream: build-py
	$(PY) tools/run_downstream.py
