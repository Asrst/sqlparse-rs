//! Rust port of python-sqlparse 0.6.0 `formatter.py` and its filters.
//!
//! Accepts a validated [`FormatOptions`] struct and a token stream (or already-
//! grouped [`Node`] tree), applies all active filters, and serializes to a
//! `String`.  The output is byte-identical to Python for all options that are
//! fully ported.
//!
//! # Ported filters (complete)
//! - `KeywordCaseFilter`     — keyword_case upper/lower/capitalize
//! - `IdentifierCaseFilter`  — identifier_case upper/lower/capitalize
//! - `TruncateStringFilter`  — truncate_strings + truncate_char
//! - `StripCommentsFilter`   — strip_comments (preserves SQL hints)
//! - `StripWhitespaceFilter` — strip_whitespace / reindent pre-step
//! - `SpacesAroundOperatorsFilter` — use_space_around_operators
//! - `StripTrailingSemicolonFilter` — used by split(strip_semicolon=True)
//! - `ReindentFilter`        — reindent with indent_width/char/wrap_after/
//!   comma_first/indent_after_first/indent_columns/compact
//! - `OutputPythonFilter`    — output_format='python'
//! - `OutputPHPFilter`       — output_format='php'
//! - `SerializerUnicode`     — trailing-whitespace trim per line
//!
//! `AlignedIndentFilter` and `RightMarginFilter` are delegated back to
//! Python for now (they are used by rare options).

use crate::grouping::{self, Node, NodeKind};
use crate::ttype;

// ──────────────────────────────────────────────────────────────────────────
// Public options struct
// ──────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct FormatOptions {
    pub keyword_case: Option<CaseMode>,
    pub identifier_case: Option<CaseMode>,
    pub strip_comments: bool,
    pub strip_whitespace: bool,
    pub reindent: bool,
    pub reindent_aligned: bool,
    pub indent_width: usize, // default 2
    pub indent_tabs: bool,
    pub indent_after_first: bool,
    pub indent_columns: bool,
    pub wrap_after: usize, // 0 = no wrap
    pub comma_first: bool,
    pub compact: bool,
    pub use_space_around_operators: bool,
    pub truncate_strings: Option<usize>,
    pub truncate_char: String, // default "[...]"
    pub output_format: Option<OutputFormat>,
    pub right_margin: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CaseMode {
    Upper,
    Lower,
    Capitalize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OutputFormat {
    Sql,
    Python,
    Php,
}

impl FormatOptions {
    pub fn new() -> Self {
        FormatOptions {
            indent_width: 2,
            truncate_char: "[...]".to_owned(),
            ..Default::default()
        }
    }

    /// Returns the indent character (space or tab).
    pub fn indent_char(&self) -> char {
        if self.indent_tabs {
            '\t'
        } else {
            ' '
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Entry point
// ──────────────────────────────────────────────────────────────────────────

/// Format SQL exactly like python-sqlparse `format()`: tokenize, apply the
/// flat preprocess filters, split into statements, then per statement:
/// group → tree filters (spaces-around-operators, strip-comments,
/// strip-whitespace, reindent) → serialize. Statement outputs are joined
/// with `""` (python `"".join(stack.run(sql))`); when reindenting, the
/// ReindentFilter inserts `\n\n` (or `\n` after a statement that already
/// ends in a newline) at the start of every statement after the first.
pub fn format_sql(sql: &str, opts: &FormatOptions) -> String {
    // Token-level split: python's formatter consumes StatementSplitter
    // output (raw token lists — trailing whitespace/comments stay in their
    // statement; only the split() API strips).
    let stmts = crate::splitter::split_tokens(sql);

    let mut out = String::new();
    let mut prev_ends_nl = false;
    for (k, stmt_tokens) in stmts.iter().enumerate() {
        out.push_str(&format_tokens(
            stmt_tokens.clone(),
            opts,
            k,
            &mut prev_ends_nl,
        ));
    }
    out
}

/// Format ONE statement's token list (python's per-statement FilterStack
/// body): flat preprocess → group → tree filters → serialize. `k` is the
/// statement index and `prev_ends_nl` carries the previous statement's
/// trailing-newline state for the ReindentFilter inter-statement prefix.
fn format_tokens(
    tokens: Vec<(&'static str, String)>,
    opts: &FormatOptions,
    k: usize,
    prev_ends_nl: &mut bool,
) -> String {
    // Step 1-2: flat preprocess (python preprocess stream filters:
    // KeywordCase, IdentifierCase, TruncateString).
    let processed: Vec<(String, String)> = tokens
        .iter()
        .map(|(tt, v)| {
            let mut val = v.clone();
            if let Some(ref kc) = opts.keyword_case {
                if ttype::is_keyword_family(tt) {
                    val = apply_case(&val, kc);
                }
            }
            if let Some(ref ic) = opts.identifier_case {
                if (*tt == ttype::NAME || *tt == ttype::STRING_SYMBOL)
                    && !val.trim_start().starts_with('"')
                {
                    val = apply_case(&val, ic);
                }
            }
            if let Some(width) = opts.truncate_strings {
                if *tt == ttype::STRING_SINGLE {
                    val = truncate_string(&val, width, &opts.truncate_char);
                }
            }
            (tt.to_string(), val)
        })
        .collect();

    let needs_grouping = opts.strip_comments
        || opts.strip_whitespace
        || opts.reindent
        || opts.reindent_aligned
        || opts.use_space_around_operators
        || opts.right_margin.is_some();

    if !needs_grouping {
        // Fast path: no grouping — flat serialize per statement, then the
        // SerializerUnicode postprocess (python applies it to EVERY yielded
        // statement, grouping or not).
        let raw: String = processed.iter().map(|(_, v)| v.as_str()).collect();
        let raw = serialize_text(&raw);
        return match opts.output_format {
            Some(ref fmt) => apply_output_format(&raw, fmt, k + 1),
            None => raw,
        };
    }

    // Step 3: grouped AST for this statement.
    let mut stmt = grouping::tokens_to_statement(processed);
    grouping::group(&mut stmt);

    // Step 4: tree filters — python stmtprocess order.
    if opts.use_space_around_operators {
        spaces_around_operators(&mut stmt);
    }
    if opts.strip_comments {
        strip_comments(&mut stmt);
    }
    if opts.strip_whitespace || opts.reindent || opts.reindent_aligned {
        strip_whitespace(&mut stmt);
    }
    if opts.reindent {
        let prefix: String = if k > 0 {
            if *prev_ends_nl {
                "\n".to_owned()
            } else {
                "\n\n".to_owned()
            }
        } else {
            String::new()
        };
        reindent(&mut stmt, opts, &prefix);
        *prev_ends_nl = stmt.value.ends_with('\n');
    }

    // Step 5: serialize = SerializerUnicode (postprocess).
    let raw = serialize_unicode(&stmt);
    match opts.output_format {
        Some(ref fmt) => apply_output_format(&raw, fmt, k + 1),
        None => raw,
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Pre-process helpers
// ──────────────────────────────────────────────────────────────────────────

fn apply_case(val: &str, mode: &CaseMode) -> String {
    match mode {
        CaseMode::Upper => val.to_uppercase(),
        CaseMode::Lower => val.to_lowercase(),
        CaseMode::Capitalize => {
            let mut chars = val.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
            }
        }
    }
}

fn truncate_string(val: &str, width: usize, trunc_char: &str) -> String {
    let (inner, quote) = if val.starts_with("''") {
        (&val[2..val.len() - 2], "''")
    } else {
        (&val[1..val.len() - 1], "'")
    };
    if inner.len() > width {
        format!("{}{}{}{}", quote, &inner[..width], trunc_char, quote)
    } else {
        val.to_owned()
    }
}

// ──────────────────────────────────────────────────────────────────────────
// StripCommentsFilter
// ──────────────────────────────────────────────────────────────────────────

fn strip_comments(node: &mut Node) {
    // Recurse into sublists first.
    let mut i = 0;
    while i < node.children.len() {
        if node.children[i].is_group() {
            let mut child = std::mem::replace(&mut node.children[i], Node::leaf(ttype::ERROR, ""));
            strip_comments(&mut child);
            node.children[i] = child;
        }
        i += 1;
    }
    // Now strip comment tokens and Comment groups (preserving SQL hints).
    let mut i = 0;
    while i < node.children.len() {
        let is_comment = is_comment_node(&node.children[i]);
        if !is_comment {
            i += 1;
            continue;
        }
        // Skip SQL hints.
        if is_hint_node(&node.children[i]) {
            i += 1;
            continue;
        }

        // Decide replacement.
        let pidx = prev_non_ws_idx(&node.children, i);
        let nidx = next_non_ws_idx(&node.children, i);
        let prev_ws = pidx
            .map(|p| node.children[p].is_whitespace())
            .unwrap_or(true);
        let next_ws = nidx
            .map(|n| node.children[n].is_whitespace())
            .unwrap_or(true);

        // Extract any trailing newline from the comment value.
        let nl = extract_trailing_nl(&node.children[i].value);

        if pidx.is_none() || nidx.is_none() || prev_ws || next_ws {
            let prev_is_open_paren = pidx
                .map(|p| node.children[p].match_token(ttype::PUNCTUATION, Some(&["("])))
                .unwrap_or(false);
            // Insert whitespace before removal if prev exists and not open paren.
            if pidx.is_some() && !prev_is_open_paren {
                let ws_node = if let Some(nl_str) = nl {
                    Node::leaf(ttype::NEWLINE, nl_str)
                } else {
                    Node::leaf(ttype::WHITESPACE, " ")
                };
                node.children.insert(i, ws_node);
                node.children.remove(i + 1);
            } else {
                node.children.remove(i);
            }
        } else {
            // Replace with whitespace.
            let ws_node = if let Some(nl_str) = nl {
                Node::leaf(ttype::NEWLINE, nl_str)
            } else {
                Node::leaf(ttype::WHITESPACE, " ")
            };
            node.children[i] = ws_node;
            i += 1;
        }
    }
    node.sync_value();
}

fn is_comment_node(n: &Node) -> bool {
    n.kind == NodeKind::Comment || n.ttype_in("Token.Comment")
}

fn is_hint_node(n: &Node) -> bool {
    n.ttype == Some(ttype::COMMENT_MULTILINE_HINT)
        || n.ttype == Some(ttype::COMMENT_SINGLE_HINT)
        || (n.kind == NodeKind::Comment
            && n.children
                .first()
                .map(|c| {
                    c.ttype == Some(ttype::COMMENT_MULTILINE_HINT)
                        || c.ttype == Some(ttype::COMMENT_SINGLE_HINT)
                })
                .unwrap_or(false))
}

fn extract_trailing_nl(val: &str) -> Option<&str> {
    // Return the trailing newline sequence (CR/LF/CRLF) if present.
    if val.ends_with("\r\n") {
        return Some("\r\n");
    }
    if val.ends_with('\n') {
        return Some("\n");
    }
    if val.ends_with('\r') {
        return Some("\r");
    }
    None
}

fn prev_non_ws_idx(_children: &[Node], idx: usize) -> Option<usize> {
    if idx == 0 {
        return None;
    }
    Some(idx - 1) // include whitespace — matches Python's skip_ws=False
}

fn next_non_ws_idx(children: &[Node], idx: usize) -> Option<usize> {
    if idx + 1 < children.len() {
        Some(idx + 1)
    } else {
        None
    }
}

// ──────────────────────────────────────────────────────────────────────────
// StripWhitespaceFilter
// ──────────────────────────────────────────────────────────────────────────

fn strip_whitespace(node: &mut Node) {
    strip_whitespace_inner(node, 0);
}

fn strip_whitespace_inner(node: &mut Node, depth: usize) {
    // Recurse first.
    let mut i = 0;
    while i < node.children.len() {
        if node.children[i].is_group() {
            let mut child = std::mem::replace(&mut node.children[i], Node::leaf(ttype::ERROR, ""));
            strip_whitespace_inner(&mut child, depth + 1);
            node.children[i] = child;
        }
        i += 1;
    }

    match node.kind {
        NodeKind::IdentifierList => stripws_identifierlist(node),
        NodeKind::Parenthesis => stripws_parenthesis(node),
        _ => stripws_default(node),
    }

    // Top-level: drop trailing whitespace.
    if depth == 0 {
        while node
            .children
            .last()
            .map(|n| n.is_whitespace())
            .unwrap_or(false)
        {
            node.children.pop();
        }
    }
    node.sync_value();
}

fn stripws_default(node: &mut Node) {
    let mut last_was_ws = false;
    let mut is_first = true;
    for tok in &mut node.children {
        if tok.is_whitespace() {
            tok.value = if last_was_ws || is_first {
                String::new()
            } else {
                " ".to_owned()
            };
        }
        last_was_ws = tok.is_whitespace();
        is_first = false;
    }
    // Remove empty whitespace tokens? Python KEEPS them (value assigned to
    // '' but the token stays) — they matter downstream: ReindentFilter's
    // _split_statements checks token_prev(skip_ws=False) and inserts a break
    // when any previous token existed, even a blanked-out one.
    node.sync_value();
}

fn stripws_identifierlist(node: &mut Node) {
    // Remove newlines immediately before a comma.
    let mut i = 0;
    while i < node.children.len() {
        if node.children[i].is_whitespace()
            && i + 1 < node.children.len()
            && node.children[i + 1].match_token(ttype::PUNCTUATION, Some(&[","]))
        {
            node.children.remove(i);
            continue;
        }
        i += 1;
    }
    stripws_default(node);
}

fn stripws_parenthesis(node: &mut Node) {
    // Drop whitespace after `(` and before `)`.
    while node.children.len() > 1 && node.children[1].is_whitespace() {
        node.children.remove(1);
    }
    while node.children.len() > 1 && node.children[node.children.len() - 2].is_whitespace() {
        let idx = node.children.len() - 2;
        node.children.remove(idx);
    }
    stripws_default(node);
}

// ──────────────────────────────────────────────────────────────────────────
// SpacesAroundOperatorsFilter
// ──────────────────────────────────────────────────────────────────────────

fn spaces_around_operators(node: &mut Node) {
    for i in 0..node.children.len() {
        if node.children[i].is_group() {
            let mut child = std::mem::replace(&mut node.children[i], Node::leaf(ttype::ERROR, ""));
            spaces_around_operators(&mut child);
            node.children[i] = child;
        }
    }
    let mut i = 0;
    while i < node.children.len() {
        let is_op = node.children[i].ttype_in(ttype::OPERATOR)
            || node.children[i].ttype_in(ttype::OPERATOR_COMPARISON);
        if !is_op {
            i += 1;
            continue;
        }

        // Ensure space after operator.
        let nidx = i + 1;
        if nidx < node.children.len() && !node.children[nidx].is_whitespace() {
            node.children
                .insert(nidx, Node::leaf(ttype::WHITESPACE, " "));
        }
        // Ensure space before operator.
        if i > 0 && !node.children[i - 1].is_whitespace() {
            node.children.insert(i, Node::leaf(ttype::WHITESPACE, " "));
            i += 1; // re-point to the operator
        }
        i += 1;
    }
    node.sync_value();
}

// ──────────────────────────────────────────────────────────────────────────
// ReindentFilter
// ──────────────────────────────────────────────────────────────────────────

struct Reindenter<'a> {
    opts: &'a FormatOptions,
    indent: isize,
    offset: isize,
    /// Running length (in chars) of the current output line — the forward
    /// equivalent of Python `_current_line_len`'s walk-back. Maintained
    /// exclusively by `process_default`'s counting pass; inserted `nl`
    /// leaves contain a break, so re-folding them is idempotent.
    line_len: usize,
    /// Ancestry of group kinds currently being processed (Python `within()`).
    stack: Vec<NodeKind>,
    /// Python `_last_func`: length of the most recent Function's name token.
    last_func_len: Option<usize>,
}

const SPLIT_WORDS: &[&str] = &[
    "FROM",
    "STRAIGHT_JOIN$",
    "JOIN$",
    "AND",
    "OR",
    "GROUP BY",
    "ORDER BY",
    "UNION",
    "VALUES",
    "SET",
    "BETWEEN",
    "EXCEPT",
    "HAVING",
    "LIMIT",
];

/// Python `_next_token` matches split words with `Token.match(regex=True)`
/// which uses `pattern.search(normalized)` — SUBSTRING matching, case
/// insensitive — over tokens whose ttype is EXACTLY `Token.Keyword` (the
/// identity check excludes DML/DDL and dotted keyword types). `JOIN$` and
/// `STRAIGHT_JOIN$` are end-anchored.
fn is_split_word(n: &Node) -> bool {
    if n.ttype != Some(ttype::KEYWORD) {
        return false;
    }
    let norm = n.normalized();
    SPLIT_WORDS.iter().any(|w| match w.strip_suffix('$') {
        Some(base) => norm.ends_with(base),
        None => norm.contains(w),
    })
}

fn reindent(stmt: &mut Node, opts: &FormatOptions, prefix: &str) {
    let mut r = Reindenter {
        opts,
        indent: if opts.indent_after_first { 1 } else { 0 },
        offset: 0,
        line_len: 0,
        stack: Vec::new(),
        last_func_len: None,
    };
    r.process(stmt);
    // Python ReindentFilter.process inserts the inter-statement prefix
    // (`\n\n`, or `\n` when the previous statement ends with a newline)
    // AFTER _process() — so the split helpers never see or eat it.
    if !prefix.is_empty() {
        stmt.children
            .insert(0, Node::leaf(ttype::WHITESPACE, prefix));
        stmt.sync_value();
    }
}

impl<'a> Reindenter<'a> {
    fn leading_ws(&self) -> isize {
        self.offset + self.indent * self.opts.indent_width as isize
    }

    fn nl(&self, extra: isize) -> Node {
        let spaces = std::cmp::max(0, self.leading_ws() + extra) as usize;
        let indent_str: String = std::iter::repeat_n(self.opts.indent_char(), spaces).collect();
        Node::leaf(ttype::WHITESPACE, &format!("\n{}", indent_str))
    }

    fn process(&mut self, node: &mut Node) {
        self.stack.push(node.kind.clone());
        match node.kind {
            NodeKind::Where => self.process_where(node),
            NodeKind::Parenthesis => self.process_parenthesis(node),
            NodeKind::Function => self.process_function(node),
            NodeKind::IdentifierList => self.process_identifierlist(node),
            NodeKind::Case => self.process_case(node),
            NodeKind::Values => self.process_values(node),
            _ => self.process_default(node, true),
        }
        self.stack.pop();
    }

    /// Update the running line length with a leaf value (break-aware).
    fn fold(&mut self, v: &str) {
        match v.rfind(['\n', '\r']) {
            Some(pos) => self.line_len = v[pos + 1..].chars().count(),
            None => self.line_len += v.chars().count(),
        }
    }

    /// Counting + recursion pass. Runs AFTER the split helpers have mutated
    /// the child list, so every child (including freshly inserted newlines)
    /// is folded exactly once, in document order. Folding is break-aware,
    /// which makes pre-folded `nl` values idempotent.
    fn process_default(&mut self, node: &mut Node, stmts: bool) {
        if stmts {
            self.split_statements(node);
        }
        self.split_kwds(node);
        for i in 0..node.children.len() {
            let mut child = std::mem::replace(&mut node.children[i], Node::leaf(ttype::ERROR, ""));
            if child.is_group() {
                self.process(&mut child);
            } else {
                self.fold(&child.value);
            }
            node.children[i] = child;
        }
        node.sync_value();
    }

    fn split_statements(&mut self, node: &mut Node) {
        let mut i = 0;
        while i < node.children.len() {
            if node.children[i].ttype_in(ttype::KEYWORD_DML)
                || node.children[i].ttype_in(ttype::KEYWORD_DDL)
            {
                // Python: prev_ = token_prev(skip_ws=False); if prev_ is
                // whitespace it is deleted, but the break is inserted whenever
                // *any* previous token existed (the deleted ws object is still
                // truthy) — including a statement-leading newline.
                let had_prev = i > 0;
                if i > 0 && node.children[i - 1].is_whitespace() {
                    node.children.remove(i - 1);
                    i = i.saturating_sub(1);
                }
                if had_prev {
                    let nl = self.nl(0);
                    node.children.insert(i, nl);
                    i += 1;
                }
            }
            i += 1;
        }
        node.sync_value();
    }

    fn split_kwds(&mut self, node: &mut Node) {
        // Python `_next_token` skips the first AND following a BETWEEN
        // (`BETWEEN a AND b` stays on one line); emulate with a flag.
        let mut skip_next_and = false;
        let mut i = 0;
        while i < node.children.len() {
            let is_split_kw = is_split_word(&node.children[i]);
            if is_split_kw {
                let norm = node.children[i].normalized();
                // Python `_next_token` NEVER returns BETWEEN: when it finds
                // one it recurses past the following AND and returns the next
                // split word after that. So BETWEEN itself is never broken;
                // the first AND after it is consumed by the recursion.
                if skip_next_and {
                    // This is the split word the BETWEEN recursion landed on.
                    skip_next_and = false;
                    if norm == "AND" {
                        i += 1;
                        continue; // consumed, no break
                    }
                    // not an AND: fall through and break normally
                }
                if norm.contains("BETWEEN") {
                    skip_next_and = true;
                    i += 1;
                    continue; // BETWEEN never breaks
                }
                // Remove preceding whitespace.
                if i > 0 && node.children[i - 1].is_whitespace() {
                    node.children.remove(i - 1);
                    i = i.saturating_sub(1);
                }
                let prev_ends_nl = i > 0
                    && (node.children[i - 1].value.ends_with('\n')
                        || node.children[i - 1].value.ends_with('\r'));
                if !prev_ends_nl {
                    let nl = self.nl(0);
                    node.children.insert(i, nl);
                    i += 1;
                }
            }
            i += 1;
        }
        node.sync_value();
    }

    fn process_where(&mut self, node: &mut Node) {
        // Insert newline before WHERE keyword.
        if let Some(wi) = node
            .children
            .iter()
            .position(|n| n.match_token(ttype::KEYWORD, Some(&["WHERE"])))
        {
            let nl = self.nl(0);
            node.children.insert(wi, nl);
            node.sync_value();
        }
        self.indent += 1;
        self.process_default(node, false);
        self.indent -= 1;
    }

    fn process_parenthesis(&mut self, node: &mut Node) {
        // Python: first = token_next_by(m=M_OPEN); if absent, nothing to do.
        let has_open = node
            .children
            .first()
            .map(|c| c.value == "(")
            .unwrap_or(false);
        if !has_open {
            return;
        }
        // Python `is_dml_dll` is truthy when a DML *or* DDL keyword is found.
        let is_dml = node
            .children
            .iter()
            .any(|c| c.ttype_in(ttype::KEYWORD_DML) || c.ttype_in(ttype::KEYWORD_DDL));
        // Python: `with indent(self, 1 if is_dml_dll else 0)` — the indent is
        // already raised when the leading newline is built.
        if is_dml {
            self.indent += 1;
            let nl = self.nl(0);
            node.children.insert(0, nl);
            node.sync_value();
            // Python's `_get_offset(first)` walks the LIVE tree, where the
            // just-inserted break already defines the current line. Pre-fold
            // it (folding a break-bearing value is idempotent).
            self.line_len = std::cmp::max(0, self.leading_ws()) as usize;
        }
        // Python: `with offset(self, self._get_offset(first) + 1)`. At entry
        // the "(" has not been folded yet, so line_len is exactly the length
        // of the line before it; for DML parens the inserted nl (which holds
        // a break) has already reset it.
        let off = self.line_len as isize - self.leading_ws() + 1;
        self.offset += off;
        self.process_default(node, !is_dml);
        self.offset -= off;
        if is_dml {
            self.indent -= 1;
        }
    }

    fn process_function(&mut self, node: &mut Node) {
        // Python: self._last_func = tlist[0] (the function-name token).
        self.last_func_len = node.children.first().map(|c| c.value.chars().count());
        self.process_default(node, true);
    }

    fn process_identifierlist(&mut self, node: &mut Node) {
        // get_identifiers(): non-whitespace, non-comma children.
        let idents: Vec<usize> = node
            .children
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.is_whitespace() && !c.match_token(ttype::PUNCTUATION, Some(&[","])))
            .map(|(i, _)| i)
            .collect();

        // Python pops the first identifier BEFORE the Function/Values branch
        // decision (when not indent_columns), so the else-branch sees the
        // popped list too.
        let popped: Vec<usize> = if self.opts.indent_columns {
            idents.clone()
        } else if idents.len() > 1 {
            idents[1..].to_vec()
        } else {
            Vec::new()
        };

        // Python `within()`: proper ancestors only. The stack holds this
        // IdentifierList as well, but it can never equal Function/Values.
        let in_function = self.stack.contains(&NodeKind::Function);
        let in_values = self.stack.contains(&NodeKind::Values);

        if !in_function && !in_values {
            if idents.is_empty() {
                self.process_default(node, false);
                return;
            }
            let num_offset: isize = if self.opts.indent_columns {
                if self.opts.indent_tabs {
                    1
                } else {
                    self.opts.indent_width as isize
                }
            } else {
                // first = next(identifiers.pop(0).flatten()); num_offset =
                // _get_offset(first) — line length before the first
                // identifier (entry line_len plus any leading inner ws).
                let first_idx = idents[0];
                let mut ll = self.line_len;
                for c in &node.children[..first_idx] {
                    ll += c.value.chars().count();
                }
                ll as isize - self.leading_ws()
            };

            self.offset += num_offset;
            let mut shift = 0usize;
            let mut position: usize = 0;
            for &idx in &popped {
                let tok_len = node.children[idx].value.chars().count();
                position += tok_len + 1;
                if position as isize > self.opts.wrap_after as isize - self.offset {
                    if self.opts.comma_first {
                        // Python: comma = token_prev(idx, skip_ws=True) — walk
                        // back over whitespace to the comma.
                        let mut comma_at = idx + shift;
                        while comma_at >= 1 && node.children[comma_at - 1].is_whitespace() {
                            comma_at -= 1;
                        }
                        if comma_at >= 1
                            && node.children[comma_at - 1]
                                .match_token(ttype::PUNCTUATION, Some(&[","]))
                        {
                            let nl = self.nl(-2);
                            node.children.insert(comma_at - 1, nl);
                            node.sync_value();
                            shift += 1;
                            // ensure whitespace after the comma
                            let after_comma = comma_at + 1;
                            let needs_ws = node
                                .children
                                .get(after_comma)
                                .map(|c| !c.is_whitespace())
                                .unwrap_or(false);
                            if needs_ws {
                                node.children
                                    .insert(after_comma, Node::leaf(ttype::WHITESPACE, " "));
                                shift += 1;
                            }
                        }
                    } else {
                        let nl = self.nl(0);
                        node.children.insert(idx + shift, nl);
                        node.sync_value();
                        shift += 1;
                    }
                    position = 0;
                }
            }
            self.offset -= num_offset;
        } else {
            // Python else-branch: within Function or Values.
            // ensure whitespace after commas
            let mut ins_ws: Vec<usize> = Vec::new();
            for i in 0..node.children.len() {
                if node.children[i].match_token(ttype::PUNCTUATION, Some(&[","])) {
                    let is_ws = node
                        .children
                        .get(i + 1)
                        .map(|c| c.is_whitespace())
                        .unwrap_or(true);
                    if !is_ws {
                        ins_ws.push(i + 1);
                    }
                }
            }
            for (k, at) in ins_ws.iter().enumerate() {
                node.children
                    .insert(at + k, Node::leaf(ttype::WHITESPACE, " "));
            }
            if !ins_ws.is_empty() {
                node.sync_value();
            }

            // Recompute identifier indices after the ws insertion.
            let idents2: Vec<usize> = node
                .children
                .iter()
                .enumerate()
                .filter(|(_, c)| {
                    !c.is_whitespace() && !c.match_token(ttype::PUNCTUATION, Some(&[","]))
                })
                .map(|(i, _)| i)
                .collect();

            let end_at: isize = self.offset
                + idents2
                    .iter()
                    .map(|&i| node.children[i].value.chars().count() as isize + 1)
                    .sum::<isize>();
            let mut adjusted: isize = 0;
            if self.opts.wrap_after > 0 && end_at > self.opts.wrap_after as isize - self.offset {
                if let Some(func_len) = self.last_func_len {
                    adjusted = -(func_len as isize) - 1;
                }
            }
            self.indent += 1; // `with indent(self)`
            if adjusted < 0 {
                if let Some(&first) = idents2.first() {
                    let nl = self.nl(0);
                    node.children.insert(first, nl);
                    node.sync_value();
                }
            }
            self.offset += adjusted; // `with offset(self, adjusted)`
            let mut shift2 = 0usize;
            let mut position: usize = 0;
            for &idx in &idents2 {
                position += node.children[idx + shift2].value.chars().count() + 1;
                if self.opts.wrap_after > 0
                    && position as isize > self.opts.wrap_after as isize - self.offset
                {
                    let nl = self.nl(0);
                    node.children.insert(idx + shift2, nl);
                    node.sync_value();
                    shift2 += 1;
                    position = 0;
                }
            }
            self.offset -= adjusted;
            self.indent -= 1;
        }

        self.process_default(node, false);
    }

    fn process_case(&mut self, node: &mut Node) {
        // Port of Python `_process_case` + `get_cases()` semantics: cond
        // includes its WHEN keyword, value includes THEN/ELSE. The first
        // pair is consumed for the offset context; remaining pairs get a
        // newline when `end_pos > wrap_after` (trivially true at the
        // default wrap_after = 0).
        let mut cases: Vec<(Option<Vec<usize>>, Vec<usize>)> = Vec::new();
        let mut mode = 1; // 1 = CONDITION, 2 = VALUE, 0 = none
        for (i, tok) in node.children.iter().enumerate() {
            if tok.match_token(ttype::KEYWORD, Some(&["CASE"])) {
                continue;
            }
            if tok.match_token(ttype::KEYWORD, Some(&["WHEN"])) {
                cases.push((Some(Vec::new()), Vec::new()));
                mode = 1;
            } else if tok.match_token(ttype::KEYWORD, Some(&["THEN"])) {
                mode = 2;
            } else if tok.match_token(ttype::KEYWORD, Some(&["ELSE"])) {
                cases.push((None, Vec::new()));
                mode = 2;
            } else if tok.match_token(ttype::KEYWORD, Some(&["END"])) {
                mode = 0;
            }
            if mode == 0 {
                continue;
            }
            if cases.is_empty() {
                cases.push((Some(Vec::new()), Vec::new()));
            }
            match mode {
                1 => {
                    if let Some(Some(cond)) = cases.last_mut().map(|c| &mut c.0) {
                        cond.push(i);
                    }
                }
                2 => {
                    cases.last_mut().unwrap().1.push(i);
                }
                _ => {}
            }
        }
        if cases.is_empty() {
            self.process_default(node, false);
            return;
        }

        // off1 = _get_offset(tlist[0]) — line before the first child
        // (the CASE keyword leaf). off2 = _get_offset(first) where
        // first = first leaf of the FIRST pair's cond[0]. Note python's
        // get_cases captures the whitespace following CASE as cond[0] of
        // the first pair ("first condition without preceding WHEN"), so
        // `first` is that whitespace token and the line before it ends
        // right after the CASE keyword.
        let off1 = self.line_len as isize - self.leading_ws();
        let cond0_idx = cases[0].0.as_ref().and_then(|c| c.first().copied());
        // python computes _get_offset(first) INSIDE `with offset(off1)`, so
        // the leading_ws subtracted here already includes off1.
        let off2 = match cond0_idx {
            Some(idx) => {
                let before: usize = node.children[..idx]
                    .iter()
                    .map(|c| c.value.chars().count())
                    .sum();
                (self.line_len + before) as isize - (self.leading_ws() + off1)
            }
            None => 0,
        };

        self.offset += off1;
        self.offset += off2;
        let mut shift = 0usize;
        for (cond, value) in cases.iter().skip(1) {
            let cond_len: usize = cond
                .as_ref()
                .map(|cs| {
                    cs.iter()
                        .map(|&i| node.children[i].value.chars().count())
                        .sum::<usize>()
                })
                .unwrap_or(0);
            let value_len: usize = value
                .iter()
                .map(|&i| node.children[i].value.chars().count())
                .sum::<usize>();
            let end_pos = self.offset + 1 + cond_len as isize + value_len as isize;
            if !self.opts.compact && end_pos > self.opts.wrap_after as isize {
                let at = match cond {
                    Some(cs) if !cs.is_empty() => cs[0],
                    _ if !value.is_empty() => value[0],
                    _ => continue,
                };
                let nl = self.nl(0);
                node.children.insert(at + shift, nl);
                node.sync_value();
                shift += 1;
            }
        }
        self.offset += "WHEN ".len() as isize; // `with offset(self, len("WHEN "))`
        self.process_default(node, false);
        self.offset -= "WHEN ".len() as isize;
        self.offset -= off2;
        // END newline: python computes it inside the off1 context but
        // outside the off2 context.
        if !self.opts.compact {
            if let Some(end_idx) = node
                .children
                .iter()
                .position(|t| t.match_token(ttype::KEYWORD, Some(&["END"])))
            {
                let nl = self.nl(0);
                node.children.insert(end_idx, nl);
                node.sync_value();
            }
        }
        self.offset -= off1;
    }

    fn process_values(&mut self, node: &mut Node) {
        // Python: nl at raw index 0, then one nl after each comma that
        // follows a Parenthesis row. `insert_after(ptoken, nl)` uses the
        // default skip_ws=True, so the newline lands AFTER the whitespace
        // following the comma; and because each `_get_offset` walk stops at
        // the previously inserted break, all rows align at the first row's
        // column. Emulated with a live line-length walk: after inserting a
        // break the walk restarts on the new line.
        let nl0 = self.nl(0);
        node.children.insert(0, nl0);
        node.sync_value();

        let mut ll: usize = std::cmp::max(0, self.leading_ws()) as usize;
        let mut last_row_ll: Option<usize> = None;
        let mut first_row_ll: Option<usize> = None;
        let mut i = 1;
        while i < node.children.len() {
            let is_group = node.children[i].is_group();
            if is_group {
                if node.children[i].kind == NodeKind::Parenthesis {
                    if first_row_ll.is_none() {
                        first_row_ll = Some(ll);
                    }
                    if last_row_ll.is_none() {
                        last_row_ll = Some(ll);
                    }
                }
                ll += node.children[i].value.chars().count();
            } else {
                let v = node.children[i].value.clone();
                match v.rfind(['\n', '\r']) {
                    Some(p) => ll = v[p + 1..].chars().count(),
                    None => ll += v.chars().count(),
                }
                if node.children[i].match_token(ttype::PUNCTUATION, Some(&[","])) {
                    if let Some(row_ll) = last_row_ll.take() {
                        if self.opts.comma_first {
                            // python: nl(offset=_get_offset(first_row) - 2)
                            // inserted BEFORE the comma.
                            let base = first_row_ll.unwrap_or(row_ll) as isize;
                            let extra = base - self.leading_ws() - 2;
                            let nl = self.nl(extra);
                            node.children.insert(i, nl);
                            node.sync_value();
                            ll = std::cmp::max(0, self.leading_ws() + extra) as usize;
                            i += 1;
                            continue;
                        } else {
                            // python: nl(_get_offset(row)) inserted AFTER the
                            // whitespace following the comma.
                            let extra = row_ll as isize - self.leading_ws();
                            let nl = self.nl(extra);
                            let mut at = i + 1;
                            while at < node.children.len() && node.children[at].is_whitespace() {
                                at += 1;
                            }
                            node.children.insert(at, nl);
                            node.sync_value();
                            ll = std::cmp::max(0, self.leading_ws() + extra) as usize;
                            i = at;
                            continue;
                        }
                    }
                }
            }
            i += 1;
        }
        self.process_default(node, false);
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Serialization
// ──────────────────────────────────────────────────────────────────────────

/// Mirrors Python `SerializerUnicode.process`: splits the flattened text on
/// UNQUOTED newlines (a newline inside a quoted string is not a break),
/// right-strips each line, and joins with `\n`. Every break — including a
/// trailing one — starts a new line, exactly like
/// `utils.split_unquoted_newlines` + `line.rstrip()`.
pub fn serialize_unicode(node: &Node) -> String {
    let text: String = node.flatten().iter().map(|n| n.value.as_str()).collect();
    serialize_text(&text)
}

/// String-level SerializerUnicode: splits on UNQUOTED newlines (a newline
/// inside a quoted string is not a break), right-strips each line, and
/// joins with `\n`. Every break — including a trailing one — starts a new
/// line, exactly like `utils.split_unquoted_newlines` + `line.rstrip()`.
pub fn serialize_text(text: &str) -> String {
    let mut lines: Vec<String> = vec![String::new()];
    let mut cur = String::new();
    let mut in_quote: Option<char> = None;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match in_quote {
            Some(q) => {
                cur.push(c);
                if c == q {
                    in_quote = None;
                }
            }
            None => match c {
                '\n' | '\r' => {
                    if !cur.is_empty() {
                        lines.last_mut().unwrap().push_str(&cur);
                        cur.clear();
                    }
                    lines.push(String::new());
                    if c == '\r' && chars.peek() == Some(&'\n') {
                        chars.next(); // \r\n is one break
                    }
                }
                '\'' | '"' => {
                    in_quote = Some(c);
                    cur.push(c);
                }
                _ => cur.push(c),
            },
        }
    }
    if !cur.is_empty() {
        lines.last_mut().unwrap().push_str(&cur);
    }
    lines
        .iter()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

// ──────────────────────────────────────────────────────────────────────────
// Output format filters
// ──────────────────────────────────────────────────────────────────────────

fn apply_output_format(sql: &str, fmt: &OutputFormat, count: usize) -> String {
    match fmt {
        OutputFormat::Sql => sql.to_owned(),
        OutputFormat::Python => output_python(sql, count),
        OutputFormat::Php => output_php(sql, count),
    }
}

fn output_python(sql: &str, count: usize) -> String {
    let varname = if count > 1 {
        format!("sql{}", count)
    } else {
        "sql".to_owned()
    };
    let has_nl = sql.trim().contains('\n');
    let escaped = sql.replace('\\', "\\\\").replace('\'', "\\'");

    if has_nl {
        let lines: Vec<&str> = escaped.split('\n').collect();
        let indent = " ".repeat(varname.len() + 4);
        let mut out = format!("{} = ('{}", varname, lines[0]);
        for line in &lines[1..] {
            if line.is_empty() {
                out.push_str(&format!("'\n{}'", indent));
            } else {
                out.push_str(&format!("'\n{}'", indent));
                out.push_str(line);
            }
        }
        out.push_str("')");
        out
    } else {
        format!("{} = '{}'", varname, escaped)
    }
}

fn output_php(sql: &str, count: usize) -> String {
    let varname = if count > 1 {
        format!("$sql{}", count)
    } else {
        "$sql".to_owned()
    };
    let has_nl = sql.trim().contains('\n');
    let escaped = sql.replace('\\', "\\\\").replace('"', "\\\"");

    if has_nl {
        let lines: Vec<&str> = escaped.split('\n').collect();
        let mut out = format!("{} = \"{}", varname, lines[0]);
        for line in &lines[1..] {
            out.push_str(&format!(" \";\n{} .= \"", varname));
            out.push_str(line);
        }
        out.push_str("\";");
        out
    } else {
        format!("{} = \"{}\";", varname, escaped)
    }
}
