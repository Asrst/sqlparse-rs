# AGENTS.md

This file provides guidance to agents when working with code in this repository.

## Architecture Constraints (Non-Obvious)

- Project name is `semetric` (CLI installable), packaged with `uv` + `pyproject.toml` (PEP 517/518 build system expected).
- The devcontainer uses `uv pip install --system --break-system-packages -e .` to make `semetric` CLI globally accessible — plan for a proper `[project.scripts]` entry in `pyproject.toml`.
- Python 3.12 is the target runtime (set by devcontainer image `python:3.12-trixie`).
- Toolchain: `uv` (deps), `ruff` (lint+format+imports), `pyright`/Pylance (types), `pytest` (tests in `tests/`).
- Node 24 is present only for developer tooling (opencode, Bob); no JS/TS application code is expected.
