# AGENTS.md

This file provides guidance to agents when working with code in this repository.

## Documentation Context (Non-Obvious)

- The repository is named `sqlparser-rs` but contains a **Python** project called `semetric` — the name is misleading and unrelated to Rust or SQL parsing.
- The devcontainer image is `python:3.12-trixie`; Node 24 is also installed (for `opencode-ai` and IBM Bob Shell — not for any application JS/TS code).
- The project is in early scaffold stage: only devcontainer config, `.gitignore`, and `LICENSE` exist. No `pyproject.toml`, `src/`, or `tests/` yet.
- IBM Bob Shell is installed globally in the devcontainer environment via `.devcontainer/setup.sh`.
