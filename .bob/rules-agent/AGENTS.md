# AGENTS.md

This file provides guidance to agents when working with code in this repository.

## Coding Rules (Non-Obvious)

- **Package manager is `uv`**, not pip. Always prefix Python tool invocations with `uv run` (e.g. `uv run pytest`, `uv run ruff`). Running `pytest` or `ruff` directly will use system/wrong binaries.
- **Install as editable with `--system --break-system-packages`** when the CLI needs to be globally available (see setup.sh). Normal `uv pip install -e .` only installs into the venv.
- Tests live in `tests/` (configured in devcontainer's `python.testing.pytestArgs`).
- Ruff handles both linting AND formatting AND import sorting — do not introduce black, isort, or flake8.
- The repo name `sqlparser-rs` is a historical artifact; the actual package/CLI is called `semetric`.
