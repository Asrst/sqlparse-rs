#!/bin/bash
set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORKSPACE_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$WORKSPACE_ROOT"

# Install Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path
source "$HOME/.cargo/env"
rustup show

# Create Python venv and install dev dependencies
python3 -m venv .venv
source .venv/bin/activate
pip install --upgrade pip
pip install maturin pytest "sqlparse==0.6.0"

# Build the Rust extension and install into the venv
maturin develop

echo ""
echo "Dev environment ready."
echo "  source \"\$HOME/.cargo/env\"   # activate Rust toolchain"
echo "  source .venv/bin/activate     # activate Python venv"
echo "  maturin develop               # rebuild after Rust changes"
echo "  pytest tests/ -v              # run all tests"
