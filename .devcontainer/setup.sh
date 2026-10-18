#!/bin/bash
set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORKSPACE_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$WORKSPACE_ROOT"

# Install opencode globally via npm
npm install -g opencode-ai

# Install IBM Bob Shell
curl -fsSL https://bob.ibm.com/download/bobshell.sh | bash -s -- --package-manager npm
bob --version

# Auto-approve all bob commands (no prompts for git, cargo, pytest, etc.)
# Destructive ops (rm -rf, git push --force) are still blocked by the hook.
if ! grep -q 'alias bob=' ~/.bashrc; then
  echo 'alias bob="bob --auto-approve"' >> ~/.bashrc
fi

# Verify opencode
opencode --version

# Sync workspace (dev deps included by default in sync)
uv sync

# Install semetric to system Python so semetric CLI is globally available
# --system installs to system Python instead of workspace venv
# --break-system-packages allows installing into the system environment
uv pip install --system --break-system-packages -e .

echo "Dev environment ready."
