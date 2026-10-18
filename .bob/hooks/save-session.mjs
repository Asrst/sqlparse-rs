#!/usr/bin/env node
/**
 * Stop hook — exports the finished Bob session to .bob/sessions/<task_id>.md
 *
 * The file is written into the workspace repo so sessions are committed to git.
 * Roles: "user" messages contain the human turn; "assistant" messages contain
 * the model turn (JSON with content + toolCalls).
 *
 * The DB path is always ~/.bob/db/bob.db.
 */

import fs from "node:fs";
import path from "node:path";

// ── 1. Read hook payload from stdin ─────────────────────────────────────────
let raw = "";
for await (const chunk of process.stdin) raw += chunk;
const payload = JSON.parse(raw);
const { session_id, cwd } = payload;

// ── 2. Open the SQLite DB with Node's built-in :memory: WAL reader ───────────
// Node ≥ 22.5 ships `node:sqlite` (experimental). Older versions need a shim.
// We use a minimal pure-JS page reader so there are no npm deps.

const DB_PATH = path.join(process.env.HOME, ".bob", "db", "bob.db");

// We'll use the `better-sqlite3` if available, otherwise fall back to a raw
// page-scan heuristic that extracts JSON blobs from the data pages.
let rows;
try {
  // Attempt to use Node's built-in sqlite (Node ≥ 22.5)
  const { DatabaseSync } = await import("node:sqlite");
  const db = new DatabaseSync(DB_PATH, { open: true });
  rows = db
    .prepare(
      `SELECT role, data, created_at
         FROM messages
        WHERE task_id = ?
        ORDER BY created_at ASC`
    )
    .all(session_id);
  db.close();
} catch {
  // Fallback: read the WAL-flushed DB pages and regex-scan for JSON blobs
  // belonging to this task_id. Works for small-to-medium DBs (< ~50 MB).
  rows = fallbackReadMessages(DB_PATH, session_id);
}

if (!rows || rows.length === 0) {
  // Nothing to save — session may have had no messages
  process.exit(0);
}

// ── 3. Render to Markdown ────────────────────────────────────────────────────
const lines = [];
const ts = new Date().toISOString();
lines.push(`# Bob Session — ${session_id}`);
lines.push(`_Exported at ${ts}_`);
lines.push(`_Workspace: ${cwd}_`);
lines.push("");

for (const row of rows) {
  const role = row.role; // "user" | "assistant" | "tool"
  const data = safeParseJSON(row.data);

  if (role === "user") {
    const content = extractText(data);
    if (!content.trim()) continue;
    lines.push(`## 🧑 User`);
    lines.push("");
    lines.push(content.trim());
    lines.push("");
  } else if (role === "assistant") {
    const content = extractText(data);
    const tools = extractToolCalls(data);
    lines.push(`## 🤖 Assistant`);
    lines.push("");
    if (content.trim()) {
      lines.push(content.trim());
      lines.push("");
    }
    for (const tc of tools) {
      lines.push(`**Tool call:** \`${tc.name}\``);
      if (tc.arguments && typeof tc.arguments === "object") {
        const brief = JSON.stringify(tc.arguments, null, 2);
        // Truncate very long tool inputs (e.g. write_file content)
        const truncated =
          brief.length > 800 ? brief.slice(0, 800) + "\n… (truncated)" : brief;
        lines.push("```json");
        lines.push(truncated);
        lines.push("```");
      }
      lines.push("");
    }
  }
}

// ── 4. Write to .bob/sessions/<task_id>.md ───────────────────────────────────
const sessionsDir = path.join(cwd, ".bob", "sessions");
fs.mkdirSync(sessionsDir, { recursive: true });
const outPath = path.join(sessionsDir, `${session_id}.md`);
fs.writeFileSync(outPath, lines.join("\n"), "utf8");

// ── Helpers ──────────────────────────────────────────────────────────────────

function safeParseJSON(str) {
  try {
    return JSON.parse(str);
  } catch {
    return { content: str };
  }
}

function extractText(data) {
  if (!data) return "";
  if (typeof data === "string") return data;
  if (typeof data.content === "string") return data.content;
  if (Array.isArray(data.content)) {
    return data.content
      .filter((b) => b.type === "text")
      .map((b) => b.text ?? "")
      .join("\n");
  }
  return "";
}

function extractToolCalls(data) {
  if (!data) return [];
  if (Array.isArray(data.toolCalls)) return data.toolCalls;
  if (Array.isArray(data.tool_calls)) return data.tool_calls;
  return [];
}

/**
 * Pure-JS fallback: scan DB pages for JSON objects containing task_id.
 * Extracts {"role":...,"data":...} pairs heuristically.
 */
function fallbackReadMessages(dbPath, taskId) {
  const buf = fs.readFileSync(dbPath);
  const text = buf.toString("latin1");
  const results = [];

  // Find all occurrences of the task_id in the file
  const needle = `"${taskId}"`;
  let pos = 0;
  while (true) {
    const idx = text.indexOf(needle, pos);
    if (idx === -1) break;
    pos = idx + 1;

    // Scan backwards to find the start of a JSON-ish blob on this page
    // Messages rows: id, task_id, role, data, created_at
    // The "data" field is a JSON string; "role" is a short string before it.
    // Strategy: look for "user" or "assistant" within ±200 bytes, then grab data.
    const window = text.slice(Math.max(0, idx - 300), idx + 500);
    const roleMatch = window.match(/"(user|assistant)"/);
    if (!roleMatch) continue;
    const role = roleMatch[1];

    // Find the next large JSON blob after the task_id occurrence
    const dataStart = text.indexOf('{"', idx);
    if (dataStart === -1 || dataStart - idx > 200) continue;

    // Attempt to read a JSON object
    let depth = 0;
    let end = dataStart;
    for (; end < Math.min(text.length, dataStart + 50000); end++) {
      if (text[end] === "{") depth++;
      else if (text[end] === "}") {
        depth--;
        if (depth === 0) break;
      }
    }
    if (depth !== 0) continue;

    const dataStr = text.slice(dataStart, end + 1);
    // Basic sanity check — must parse as JSON
    let parsed;
    try {
      parsed = JSON.parse(dataStr);
    } catch {
      continue;
    }
    results.push({ role, data: dataStr, created_at: 0 });
  }

  return results;
}
