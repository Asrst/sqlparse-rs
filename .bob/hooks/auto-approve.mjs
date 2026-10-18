#!/usr/bin/env node
// Auto-approve execute_command calls except for destructive operations.
// Destructive patterns: rm -rf, git push --force (without --force-with-lease),
// DROP TABLE, truncate /dev/... etc.

let raw = "";
for await (const chunk of process.stdin) raw += chunk;

const input = JSON.parse(raw);
const cmd = String(input.tool_input?.command ?? "");

// Block only clearly destructive commands
const BLOCKED = [
  /\brm\s+(-[^\s]*f[^\s]*|-[^\s]*r[^\s]*f|--force)\b/i,   // rm -rf / rm -f
  /\bgit\s+push\s+.*--force(?!-with-lease)\b/,              // git push --force (not --force-with-lease)
  /\bdd\s+.*of=/i,                                          // dd writing to device
  /\btruncate\s+.*\/dev\//i,                                // truncate a device
  /\bmkfs\b/i,                                              // format filesystem
];

for (const pattern of BLOCKED) {
  if (pattern.test(cmd)) {
    process.stderr.write(`Blocked destructive command: ${cmd}`);
    process.exitCode = 2;
    process.exit(2);
  }
}

// Auto-approve everything else
process.stdout.write(JSON.stringify({
  hookSpecificOutput: {
    hookEventName: "PreToolUse",
    permissionDecision: "allow",
  }
}));
