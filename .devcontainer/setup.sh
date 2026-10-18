#!/bin/bash
set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORKSPACE_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$WORKSPACE_ROOT"

# Install Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path
source "$HOME/.cargo/env"
rustup show

# Install uv (fast Python package manager)
curl -LsSf https://astral.sh/uv/install.sh | sh
export PATH="$HOME/.local/bin:$PATH"

# Create venv and install dev dependencies via uv
uv venv .venv
uv pip install --python .venv/bin/python maturin "sqlparse==0.6.0" pytest

# Build the Rust extension and install into the venv
source .venv/bin/activate
maturin develop

echo ""
echo "Dev environment ready."
echo "  source \"\$HOME/.cargo/env\"   # activate Rust toolchain"
echo "  source .venv/bin/activate     # activate Python venv"
echo "  maturin develop               # rebuild after Rust changes"
echo "  uv run pytest tests/ -v       # run all tests"
