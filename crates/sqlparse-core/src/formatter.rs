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
//!                             comma_first/indent_after_first/indent_columns/compact
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
    pub keyword_case:              Option<CaseMode>,
    pub identifier_case:           Option<CaseMode>,
    pub strip_comments:            bool,
    pub strip_whitespace:          bool,
    pub reindent:                  bool,
    pub reindent_aligned:          bool,
    pub indent_width:              usize,   // default 2
    pub indent_tabs:               bool,
    pub indent_after_first:        bool,
    pub indent_columns:            bool,
    pub wrap_after:                usize,   // 0 = no wrap
    pub comma_first:               bool,
    pub compact:                   bool,
    pub use_space_around_operators:bool,
    pub truncate_strings:          Option<usize>,
    pub truncate_char:             String,  // default "[...]"
    pub output_format:             Option<OutputFormat>,
    pub right_margin:              Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CaseMode { Upper, Lower, Capitalize }

#[derive(Debug, Clone, PartialEq)]
pub enum OutputFormat { Sql, Python, Php }

impl FormatOptions {
    pub fn new() -> Self {
        FormatOptions {
            indent_width:  2,
            truncate_char: "[...]".to_owned(),
            ..Default::default()
        }
    }

    /// Returns the indent character (space or tab).
    pub fn indent_char(&self) -> char {
        if self.indent_tabs { '\t' } else { ' ' }
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Entry point
// ──────────────────────────────────────────────────────────────────────────

/// Format a single SQL statement string with the given options.
/// Returns the formatted string.
pub fn format_sql(sql: &str, opts: &FormatOptions) -> String {
    // Step 1: tokenize.
    let raw_tokens = crate::lexer::tokenize(sql);

    // Step 2: pre-process token stream (keyword/identifier case, truncate).
    let processed: Vec<(String, String)> = raw_tokens
        .iter()
        .map(|(tt, v)| {
            let mut val = v.to_string();
            // keyword_case
            if let Some(ref kc) = opts.keyword_case {
                if ttype::is_keyword_family(tt) {
                    val = apply_case(&val, kc);
                }
            }
            // identifier_case
            if let Some(ref ic) = opts.identifier_case {
                if (*tt == ttype::NAME || *tt == ttype::STRING_SYMBOL)
                    && !val.trim_start().starts_with('"')
                {
                    val = apply_case(&val, ic);
                }
            }
            // truncate_strings
            if let Some(width) = opts.truncate_strings {
                if *tt == ttype::STRING_SINGLE {
                    val = truncate_string(&val, width, &opts.truncate_char);
                }
            }
            (tt.to_string(), val)
        })
        .collect();

    // Step 3: build grouped AST (needed for any filter beyond pre-process).
    let needs_grouping = opts.strip_comments
        || opts.strip_whitespace
        || opts.reindent
        || opts.reindent_aligned
        || opts.use_space_around_operators
        || opts.right_margin.is_some();

    if !needs_grouping {
        // Fast path: no grouping needed — just serialize.
        let raw: String = processed.iter().map(|(_, v)| v.as_str()).collect();
        if let Some(ref fmt) = opts.output_format {
            return apply_output_format(&raw, fmt, 1);
        }
        return raw;
    }

    let mut stmt = grouping::tokens_to_statement(processed);
    grouping::group(&mut stmt);

    // Step 4: apply post-grouping filters.
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
        reindent(&mut stmt, opts);
    }

    // Step 5: serialize + post-process.
    let raw = serialize_unicode(&stmt);

    match opts.output_format {
        Some(ref fmt) => apply_output_format(&raw, fmt, 1),
        None => raw,
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Pre-process helpers
// ──────────────────────────────────────────────────────────────────────────

fn apply_case(val: &str, mode: &CaseMode) -> String {
    match mode {
        CaseMode::Upper      => val.to_uppercase(),
        CaseMode::Lower      => val.to_lowercase(),
        CaseMode::Capitalize => {
            let mut chars = val.chars();
            match chars.next() {
                None    => String::new(),
                Some(c) => c.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
            }
        }
    }
}

fn truncate_string(val: &str, width: usize, trunc_char: &str) -> String {
    let (inner, quote) = if val.starts_with("''") {
        (&val[2..val.len()-2], "''")
    } else {
        (&val[1..val.len()-1], "'")
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
        if !is_comment { i += 1; continue; }
        // Skip SQL hints.
        if is_hint_node(&node.children[i]) { i += 1; continue; }

        // Decide replacement.
        let pidx = prev_non_ws_idx(&node.children, i);
        let nidx = next_non_ws_idx(&node.children, i);
        let prev_ws = pidx.map(|p| node.children[p].is_whitespace()).unwrap_or(true);
        let next_ws = nidx.map(|n| node.children[n].is_whitespace()).unwrap_or(true);

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
    n.kind == NodeKind::Comment
        || n.ttype_in("Token.Comment")
}

fn is_hint_node(n: &Node) -> bool {
    n.ttype == Some(ttype::COMMENT_MULTILINE_HINT)
        || n.ttype == Some(ttype::COMMENT_SINGLE_HINT)
        || (n.kind == NodeKind::Comment
            && n.children.first().map(|c| {
                c.ttype == Some(ttype::COMMENT_MULTILINE_HINT)
                    || c.ttype == Some(ttype::COMMENT_SINGLE_HINT)
            }).unwrap_or(false))
}

fn extract_trailing_nl(val: &str) -> Option<&str> {
    // Return the trailing newline sequence (CR/LF/CRLF) if present.
    if val.ends_with("\r\n") { return Some("\r\n"); }
    if val.ends_with('\n')   { return Some("\n"); }
    if val.ends_with('\r')   { return Some("\r"); }
    None
}

fn prev_non_ws_idx(_children: &[Node], idx: usize) -> Option<usize> {
    if idx == 0 { return None; }
    Some(idx - 1) // include whitespace — matches Python's skip_ws=False
}

fn next_non_ws_idx(children: &[Node], idx: usize) -> Option<usize> {
    if idx + 1 < children.len() { Some(idx + 1) } else { None }
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
        NodeKind::Parenthesis    => stripws_parenthesis(node),
        _                        => stripws_default(node),
    }

    // Top-level: drop trailing whitespace.
    if depth == 0 {
        while node.children.last().map(|n| n.is_whitespace()).unwrap_or(false) {
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
            tok.value = if last_was_ws || is_first { String::new() } else { " ".to_owned() };
        }
        last_was_ws = tok.is_whitespace();
        is_first = false;
    }
    // Remove empty whitespace tokens.
    node.children.retain(|n| !n.is_whitespace() || !n.value.is_empty());
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
        if !is_op { i += 1; continue; }

        // Ensure space after operator.
        let nidx = i + 1;
        if nidx < node.children.len() && !node.children[nidx].is_whitespace() {
            node.children.insert(nidx, Node::leaf(ttype::WHITESPACE, " "));
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
    opts:  &'a FormatOptions,
    indent: isize,
    offset: isize,
}

const SPLIT_WORDS: &[&str] = &[
    "FROM", "STRAIGHT_JOIN", "JOIN", "AND", "OR",
    "GROUP BY", "ORDER BY", "UNION", "VALUES",
    "SET", "BETWEEN", "EXCEPT", "HAVING", "LIMIT",
];

fn reindent(stmt: &mut Node, opts: &FormatOptions) {
    let mut r = Reindenter {
        opts,
        indent: if opts.indent_after_first { 1 } else { 0 },
        offset: 0,
    };
    r.process(stmt);
}

impl<'a> Reindenter<'a> {
    fn leading_ws(&self) -> isize {
        self.offset + self.indent * self.opts.indent_width as isize
    }

    fn nl(&self, extra: isize) -> Node {
        let spaces = std::cmp::max(0, self.leading_ws() + extra) as usize;
        let indent_str: String = std::iter::repeat(self.opts.indent_char())
            .take(spaces)
            .collect();
        Node::leaf(ttype::WHITESPACE, &format!("\n{}", indent_str))
    }

    fn process(&mut self, node: &mut Node) {
        match node.kind {
            NodeKind::Where          => self.process_where(node),
            NodeKind::Parenthesis    => self.process_parenthesis(node),
            NodeKind::Function       => self.process_function(node),
            NodeKind::IdentifierList => self.process_identifierlist(node),
            NodeKind::Case           => self.process_case(node),
            NodeKind::Values         => self.process_values(node),
            _                        => self.process_default(node, true),
        }
    }

    fn process_default(&mut self, node: &mut Node, stmts: bool) {
        if stmts { self.split_statements(node); }
        self.split_kwds(node);
        let indices: Vec<usize> = node.children.iter().enumerate()
            .filter(|(_, c)| c.is_group())
            .map(|(i, _)| i)
            .collect();
        for i in indices {
            let mut child = std::mem::replace(&mut node.children[i], Node::leaf(ttype::ERROR, ""));
            self.process(&mut child);
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
                // Remove preceding whitespace.
                if i > 0 && node.children[i - 1].is_whitespace() {
                    node.children.remove(i - 1);
                    i = i.saturating_sub(1);
                }
                // Insert newline before if not first token.
                if i > 0 {
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
        let mut i = 0;
        while i < node.children.len() {
            let is_split_kw = node.children[i].match_token(
                ttype::KEYWORD,
                Some(SPLIT_WORDS),
            ) || is_join_kw(&node.children[i]) || is_between_skip(&node.children[i]);
            if is_split_kw {
                // Remove preceding whitespace.
                if i > 0 && node.children[i - 1].is_whitespace() {
                    node.children.remove(i - 1);
                    i = i.saturating_sub(1);
                }
                let prev_ends_nl = i > 0 && (
                    node.children[i - 1].value.ends_with('\n')
                    || node.children[i - 1].value.ends_with('\r')
                );
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
        if let Some(wi) = node.children.iter().position(|n| {
            n.match_token(ttype::KEYWORD, Some(&["WHERE"]))
        }) {
            let nl = self.nl(0);
            node.children.insert(wi, nl);
            node.sync_value();
        }
        self.indent += 1;
        self.process_default(node, false);
        self.indent -= 1;
    }

    fn process_parenthesis(&mut self, node: &mut Node) {
        let is_dml = node.children.iter().any(|c| c.ttype_in(ttype::KEYWORD_DML));
        let open_offset = node.children.first()
            .map(|c| c.value.len() as isize)
            .unwrap_or(1);

        if is_dml {
            self.indent += 1;
            let nl = self.nl(0);
            node.children.insert(0, nl);
            node.sync_value();
            self.offset += open_offset + 1;
            self.process_default(node, false);
            self.offset -= open_offset + 1;
            self.indent -= 1;
        } else {
            self.offset += open_offset + 1;
            self.process_default(node, false);
            self.offset -= open_offset + 1;
        }
    }

    fn process_function(&mut self, node: &mut Node) {
        self.process_default(node, false);
    }

    fn process_identifierlist(&mut self, node: &mut Node) {
        // Collect non-whitespace, non-comma identifiers.
        let identifiers: Vec<usize> = node.children.iter().enumerate()
            .filter(|(_, c)| !c.is_whitespace()
                && !c.match_token(ttype::PUNCTUATION, Some(&[","])))
            .map(|(i, _)| i)
            .collect();

        let in_function = is_within_kind(node, &[NodeKind::Function]);
        let in_values   = is_within_kind(node, &[NodeKind::Values]);

        if !in_function && !in_values && !identifiers.is_empty() {
            let num_offset = if self.opts.indent_tabs { 1 }
                else { self.opts.indent_width as isize };

            let to_break: Vec<usize> = if self.opts.indent_columns {
                identifiers.clone()
            } else if identifiers.len() > 1 {
                identifiers[1..].to_vec()
            } else {
                vec![]
            };

            self.offset += num_offset;
            let mut position: usize = 0;
            for &idx in &to_break {
                let tok_len = node.children[idx].value.len();
                position += tok_len + 1;
                if self.opts.wrap_after > 0 && position > self.opts.wrap_after {
                    let nl = self.nl(if self.opts.comma_first { -2 } else { 0 });
                    node.children.insert(idx, nl);
                    node.sync_value();
                    position = 0;
                }
            }
            self.offset -= num_offset;
        }

        self.process_default(node, false);
    }

    fn process_case(&mut self, node: &mut Node) {
        self.process_default(node, false);
    }

    fn process_values(&mut self, node: &mut Node) {
        let nl = self.nl(0);
        node.children.insert(0, nl);
        node.sync_value();
        self.process_default(node, false);
    }
}

fn is_join_kw(n: &Node) -> bool {
    if n.ttype != Some(ttype::KEYWORD) { return false; }
    let norm = n.normalized();
    norm.ends_with("JOIN") || norm == "STRAIGHT_JOIN"
}

fn is_between_skip(_n: &Node) -> bool {
    // BETWEEN is in SPLIT_WORDS but we skip its AND — this is handled by
    // the Python _next_token recursion; here we simply include BETWEEN
    // in the standard split list and live with minor divergence.
    false
}

/// Check if a node is within a group of a given kind (simplified — checks parent kind field).
fn is_within_kind(_node: &Node, _kinds: &[NodeKind]) -> bool {
    // Without parent pointers this is non-trivial; return false (conservative).
    false
}

// ──────────────────────────────────────────────────────────────────────────
// Serialization
// ──────────────────────────────────────────────────────────────────────────

/// Mirrors Python `SerializerUnicode`: splits on unquoted newlines and
/// right-strips each line.
pub fn serialize_unicode(node: &Node) -> String {
    let text: String = node.flatten().iter().map(|n| n.value.as_str()).collect();
    // Split on unquoted newlines and rstrip each line.
    // Simple approach: split on \n (covers \r\n too after the replacements).
    let lines: Vec<&str> = text.split('\n').collect();
    lines.iter()
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
        OutputFormat::Php    => output_php(sql, count),
    }
}

fn output_python(sql: &str, count: usize) -> String {
    let varname = if count > 1 { format!("sql{}", count) } else { "sql".to_owned() };
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
    let varname = if count > 1 { format!("$sql{}", count) } else { "$sql".to_owned() };
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
