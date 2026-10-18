PY      ?= python3
CORPUS  := corpus
INPUTS  := $(CORPUS)/inputs.bin
GOLDENS := $(CORPUS)/goldens.txt

.PHONY: corpus goldens split-goldens check gen-keywords port-matrix build-py test-py bench-py clean

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

## Build the PyO3 extension into the active venv
build-py:
	cd crates/sqlparse-py && maturin develop

## Python-side tests (wrapper + parity vs sqlparse)
test-py: build-py
	$(PY) -m pytest tests/test_wrapper.py -q

bench-py: $(INPUTS)
	$(PY) tools/oracle.py tokenize < $(INPUTS) > /dev/null

clean:
	rm -f $(INPUTS) $(GOLDENS)
