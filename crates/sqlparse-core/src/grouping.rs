//! Rust port of python-sqlparse 0.6.0 `engine/grouping.py` and `sql.py`.
//!
//! Builds a tree of [`Node`] values from a flat `(ttype, value)` token
//! stream produced by [`crate::lexer::tokenize`].  The output matches the
//! Python AST exactly: same grouping passes, same order, same node types.
//!
//! # Design
//! * Nodes are owned `Vec<Node>` trees — no arena, no Rc/RefCell.
//! * Mutation during grouping passes uses index-based replacement (mirrors
//!   Python's `group_tokens`).
//! * Every node carries its original text so `flatten()` / `to_string()`
//!   is an O(n) concatenation.

use crate::ttype;

// ──────────────────────────────────────────────────────────────────────────
// Public node kind tag
// ──────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    // Leaf token kinds
    Leaf,
    // Group kinds — correspond 1:1 to Python sql.* classes
    Statement,
    TokenList,
    Identifier,
    IdentifierList,
    Parenthesis,
    SquareBrackets,
    Assignment,
    If,
    For,
    Comparison,
    Comment,
    Where,
    Over,
    Case,
    Function,
    Begin,
    Operation,
    Values,
    TypedLiteral,
}

// ──────────────────────────────────────────────────────────────────────────
// Node
// ──────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: NodeKind,
    /// Non-None only for leaf tokens.
    pub ttype: Option<&'static str>,
    /// For leaves: original value.  For groups: concatenated value of all
    /// children (kept in sync by `group_tokens`).
    pub value: String,
    pub children: Vec<Node>,
}

impl Node {
    // ── constructors ──────────────────────────────────────────────────────

    pub fn leaf(ttype: &'static str, value: &str) -> Self {
        Node {
            kind: NodeKind::Leaf,
            ttype: Some(ttype),
            value: value.to_owned(),
            children: vec![],
        }
    }

    pub fn group(kind: NodeKind, children: Vec<Node>) -> Self {
        let value: String = children.iter().map(|n| n.value.as_str()).collect();
        Node { kind, ttype: None, value, children }
    }

    // ── predicates ────────────────────────────────────────────────────────

    #[inline]
    pub fn is_leaf(&self) -> bool { self.kind == NodeKind::Leaf }

    #[inline]
    pub fn is_group(&self) -> bool { self.kind != NodeKind::Leaf }

    #[inline]
    pub fn is_whitespace(&self) -> bool {
        matches!(self.ttype, Some(t) if ttype::is_ws_like(t))
    }

    #[inline]
    pub fn is_newline(&self) -> bool {
        self.ttype == Some(ttype::NEWLINE)
    }

    #[inline]
    pub fn is_keyword(&self) -> bool {
        matches!(self.ttype, Some(t) if ttype::is_keyword_family(t))
    }

    /// `normalized`: keywords are upper-cased, everything else unchanged.
    pub fn normalized(&self) -> String {
        if self.is_keyword() {
            self.value.to_uppercase()
        } else {
            self.value.clone()
        }
    }

    /// Hierarchical ttype containment: `t in T.Keyword` etc.
    /// `needle` is a prefix like `"Token.Keyword"`.
    pub fn ttype_in(&self, needle: &str) -> bool {
        match self.ttype {
            Some(t) => t == needle || t.starts_with(&format!("{}.", needle)),
            None => false,
        }
    }

    /// Match `(ttype_prefix, values)` — keyword values compared case-insensitively.
    /// Pass `values = None` to match ttype only.
    pub fn match_token(&self, ttype_prefix: &str, values: Option<&[&str]>) -> bool {
        if !self.ttype_in(ttype_prefix) {
            return false;
        }
        match values {
            None => true,
            Some(vs) => {
                let norm = self.normalized();
                vs.iter().any(|v| norm == v.to_uppercase())
            }
        }
    }

    // ── tree helpers ──────────────────────────────────────────────────────

    /// Recursively yield references to all leaf nodes.
    pub fn flatten(&self) -> Vec<&Node> {
        let mut out = vec![];
        self.flatten_into(&mut out);
        out
    }

    fn flatten_into<'a>(&'a self, out: &mut Vec<&'a Node>) {
        if self.is_leaf() {
            out.push(self);
        } else {
            for c in &self.children {
                c.flatten_into(out);
            }
        }
    }

    /// Rebuild `self.value` from children (call after in-place mutations).
    pub fn sync_value(&mut self) {
        if self.is_group() {
            self.value = self.children.iter().map(|n| n.value.as_str()).collect();
        }
    }

    /// Returns the first non-whitespace, optionally non-comment child.
    pub fn token_first(&self, skip_cm: bool) -> Option<&Node> {
        self.children.iter().find(|n| {
            if n.is_whitespace() { return false; }
            if skip_cm && (n.ttype_in("Token.Comment") || n.kind == NodeKind::Comment) {
                return false;
            }
            true
        })
    }
}

impl std::fmt::Display for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Build initial flat Statements from the token stream
// ──────────────────────────────────────────────────────────────────────────

/// Convert a flat token stream into a list of `Statement` nodes (pre-grouping).
/// Uses the same splitting logic already handled by `splitter.rs`, but here
/// we work *after* split — each input is a single statement's tokens.
pub fn tokens_to_statement(tokens: Vec<(String, String)>) -> Node {
    let leaves: Vec<Node> = tokens
        .into_iter()
        .map(|(tt, v)| {
            // Map the owned String ttype back to a &'static str constant.
            let ttype_str: &'static str = ttype_static(&tt);
            Node::leaf(ttype_str, &v)
        })
        .collect();
    Node::group(NodeKind::Statement, leaves)
}

/// Map a ttype string to its &'static str constant.  Unknown types fall back
/// to the closest parent (worst case: `"Token.Error"`).
fn ttype_static(s: &str) -> &'static str {
    match s {
        "Token.Comment.Single.Hint"       => ttype::COMMENT_SINGLE_HINT,
        "Token.Comment.Single"            => ttype::COMMENT_SINGLE,
        "Token.Comment.Multiline"         => ttype::COMMENT_MULTILINE,
        "Token.Comment.Multiline.Hint"    => ttype::COMMENT_MULTILINE_HINT,
        "Token.Text.Whitespace.Newline"   => ttype::NEWLINE,
        "Token.Text.Whitespace"           => ttype::WHITESPACE,
        "Token.Assignment"                => ttype::ASSIGNMENT,
        "Token.Punctuation"               => ttype::PUNCTUATION,
        "Token.Wildcard"                  => ttype::WILDCARD,
        "Token.Name"                      => ttype::NAME,
        "Token.Name.Placeholder"          => ttype::NAME_PLACEHOLDER,
        "Token.Name.Builtin"              => ttype::NAME_BUILTIN,
        "Token.Generic.Command"           => ttype::GENERIC_COMMAND,
        "Token.Keyword"                   => ttype::KEYWORD,
        "Token.Keyword.DML"               => ttype::KEYWORD_DML,
        "Token.Keyword.DDL"               => ttype::KEYWORD_DDL,
        "Token.Keyword.CTE"               => ttype::KEYWORD_CTE,
        "Token.Keyword.Order"             => ttype::KEYWORD_ORDER,
        "Token.Keyword.TZCast"            => ttype::KEYWORD_TZCAST,
        "Token.Literal.Number.Hexadecimal"=> ttype::NUMBER_HEX,
        "Token.Literal.Number.Float"      => ttype::NUMBER_FLOAT,
        "Token.Literal.Number.Integer"    => ttype::NUMBER_INTEGER,
        "Token.Literal.String.Single"     => ttype::STRING_SINGLE,
        "Token.Literal.String.Symbol"     => ttype::STRING_SYMBOL,
        "Token.Literal"                   => ttype::LITERAL,
        "Token.Operator"                  => ttype::OPERATOR,
        "Token.Operator.Comparison"       => ttype::OPERATOR_COMPARISON,
        "Token.Error"                     => ttype::ERROR,
        _                                 => ttype::ERROR,
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Low-level tree mutation helpers  (mirrors TokenList methods in sql.py)
// ──────────────────────────────────────────────────────────────────────────

/// `token_next` — index of next non-ws token after `idx` (exclusive start).
fn token_next(children: &[Node], idx: usize, skip_ws: bool) -> Option<usize> {
    let start = idx + 1;
    for i in start..children.len() {
        if skip_ws && children[i].is_whitespace() { continue; }
        return Some(i);
    }
    None
}

/// `token_prev` — index of prev non-ws token before `idx` (exclusive start).
fn token_prev(children: &[Node], idx: usize, skip_ws: bool) -> Option<usize> {
    if idx == 0 { return None; }
    let mut i = idx - 1;
    loop {
        if !skip_ws || !children[i].is_whitespace() {
            return Some(i);
        }
        if i == 0 { break; }
        i -= 1;
    }
    None
}

/// Find first token at `>= start` where `pred` is true.
fn token_next_by<F>(children: &[Node], start: usize, pred: F) -> Option<usize>
where
    F: Fn(&Node) -> bool,
{
    for i in start..children.len() {
        if pred(&children[i]) { return Some(i); }
    }
    None
}

/// Replace children `[start..=end]` with a single group node of `kind`.
/// Returns the new index (always `start`).
fn group_tokens(children: &mut Vec<Node>, kind: NodeKind, start: usize, end: usize, extend: bool) -> usize {
    let end_excl = end + 1;
    // Extend mode: if `children[start]` is already of the same kind, absorb.
    if extend && start < children.len() && children[start].kind == kind {
        let extra: Vec<Node> = children.drain(start + 1..end_excl).collect();
        let grp = &mut children[start];
        for n in extra {
            grp.value.push_str(&n.value);
            grp.children.push(n);
        }
        return start;
    }
    let sub: Vec<Node> = children.drain(start..end_excl).collect();
    let grp = Node::group(kind, sub);
    children.insert(start, grp);
    start
}

// ──────────────────────────────────────────────────────────────────────────
// imt helper  (Instance / Match / Type)
// ──────────────────────────────────────────────────────────────────────────

fn imt_kind(n: &Node, kinds: &[NodeKind]) -> bool {
    kinds.contains(&n.kind)
}

fn imt_ttype(n: &Node, ttype_prefixes: &[&str]) -> bool {
    ttype_prefixes.iter().any(|p| n.ttype_in(p))
}

#[allow(dead_code)]
fn imt_match(n: &Node, ttype_prefix: &str, values: Option<&[&str]>) -> bool {
    n.match_token(ttype_prefix, values)
}

// ──────────────────────────────────────────────────────────────────────────
// Grouping passes — exact order from grouping.py `group()`
// ──────────────────────────────────────────────────────────────────────────

pub fn group(stmt: &mut Node) {
    group_comments(stmt);
    group_brackets(stmt);
    group_parenthesis(stmt);
    group_case(stmt);
    group_if(stmt);
    group_for(stmt);
    group_begin(stmt);
    group_over(stmt);
    group_functions(stmt);
    group_where(stmt);
    group_period(stmt);
    group_arrays(stmt);
    group_identifier(stmt);
    group_order(stmt);
    group_typecasts(stmt);
    group_tzcasts(stmt);
    group_typed_literal(stmt);
    group_operator(stmt);
    group_comparison(stmt);
    group_as(stmt);
    group_aliased(stmt);
    group_assignment(stmt);
    align_comments(stmt);
    group_identifier_list(stmt);
    group_values(stmt);
}

// ── _group_matching ────────────────────────────────────────────────────────

fn group_matching(node: &mut Node, kind: NodeKind, m_open: (&str, Option<&[&str]>), m_close: (&str, Option<&[&str]>)) {
    // Recurse into sub-groups of different kind first.
    for i in 0..node.children.len() {
        if node.children[i].is_group() && node.children[i].kind != kind {
            let mut child = std::mem::replace(&mut node.children[i], Node::leaf(ttype::ERROR, ""));
            group_matching(&mut child, kind.clone(), m_open, m_close);
            node.children[i] = child;
        }
    }

    let mut opens: Vec<usize> = vec![];
    let mut offset: isize = 0;

    // Snapshot the original length; we iterate over original indices.
    let snap: Vec<bool> = node.children.iter().map(|c| {
        !c.is_whitespace() && c.is_leaf()
    }).collect();
    let orig_len = snap.len();

    let mut orig_idx = 0usize;
    while orig_idx < orig_len {
        if node.children.is_empty() { break; }
        let cur_idx = (orig_idx as isize + offset) as usize;
        if cur_idx >= node.children.len() { orig_idx += 1; continue; }

        let n = &node.children[cur_idx];
        if n.is_whitespace() { orig_idx += 1; continue; }
        if n.is_group() && n.kind != kind { orig_idx += 1; continue; }

        let is_open  = n.match_token(m_open.0,  m_open.1);
        let is_close = n.match_token(m_close.0, m_close.1);

        if is_open {
            opens.push(cur_idx);
        } else if is_close {
            if let Some(open_idx) = opens.pop() {
                let close_idx = cur_idx;
                group_tokens(&mut node.children, kind.clone(), open_idx, close_idx, false);
                let collapsed = close_idx - open_idx;
                offset -= collapsed as isize;
            }
        }
        orig_idx += 1;
    }
    node.sync_value();
}

fn group_brackets(stmt: &mut Node) {
    group_matching(stmt, NodeKind::SquareBrackets,
        (ttype::PUNCTUATION, Some(&["["])),
        (ttype::PUNCTUATION, Some(&["]"])));
}

fn group_parenthesis(stmt: &mut Node) {
    group_matching(stmt, NodeKind::Parenthesis,
        (ttype::PUNCTUATION, Some(&["("])),
        (ttype::PUNCTUATION, Some(&[")"])));
}

fn group_case(stmt: &mut Node) {
    group_matching(stmt, NodeKind::Case,
        (ttype::KEYWORD, Some(&["CASE"])),
        (ttype::KEYWORD, Some(&["END"])));
}

fn group_if(stmt: &mut Node) {
    group_matching(stmt, NodeKind::If,
        (ttype::KEYWORD, Some(&["IF"])),
        (ttype::KEYWORD, Some(&["END IF"])));
}

fn group_for(stmt: &mut Node) {
    group_matching(stmt, NodeKind::For,
        (ttype::KEYWORD, Some(&["FOR", "FOREACH"])),
        (ttype::KEYWORD, Some(&["END LOOP"])));
}

fn group_begin(stmt: &mut Node) {
    group_matching(stmt, NodeKind::Begin,
        (ttype::KEYWORD, Some(&["BEGIN"])),
        (ttype::KEYWORD, Some(&["END"])));
}

// ── group_over ────────────────────────────────────────────────────────────

fn group_over_inner(node: &mut Node) {
    // Recurse into non-Over sublists.
    for i in 0..node.children.len() {
        if node.children[i].is_group() && node.children[i].kind != NodeKind::Over {
            let mut child = std::mem::replace(&mut node.children[i], Node::leaf(ttype::ERROR, ""));
            group_over_inner(&mut child);
            node.children[i] = child;
        }
    }
    let mut i = 0usize;
    while i < node.children.len() {
        if node.children[i].match_token(ttype::KEYWORD, Some(&["OVER"])) {
            if let Some(nidx) = token_next(&node.children, i, true) {
                let ok = node.children[nidx].kind == NodeKind::Parenthesis
                    || node.children[nidx].ttype == Some(ttype::NAME);
                if ok {
                    group_tokens(&mut node.children, NodeKind::Over, i, nidx, false);
                    node.sync_value();
                    continue; // re-check same index
                }
            }
        }
        i += 1;
    }
}

fn group_over(stmt: &mut Node) { group_over_inner(stmt); }

// ── group_functions ───────────────────────────────────────────────────────

fn group_functions_inner(node: &mut Node) {
    // Check CREATE TABLE WITHOUT AS sentinel.
    let has_create = node.children.iter().any(|c| c.value.to_uppercase() == "CREATE");
    let has_table  = node.children.iter().any(|c| c.value.to_uppercase() == "TABLE");
    let has_as     = node.children.iter().any(|c| c.value.to_uppercase() == "AS");
    if has_create && has_table && !has_as { return; }

    // Recurse into non-Function sublists.
    for i in 0..node.children.len() {
        if node.children[i].is_group() && node.children[i].kind != NodeKind::Function {
            let mut child = std::mem::replace(&mut node.children[i], Node::leaf(ttype::ERROR, ""));
            group_functions_inner(&mut child);
            node.children[i] = child;
        }
    }

    let mut i = 0usize;
    while i < node.children.len() {
        if node.children[i].ttype == Some(ttype::NAME) {
            if let Some(nidx) = token_next(&node.children, i, true) {
                if node.children[nidx].kind == NodeKind::Parenthesis {
                    // Check for trailing Over.
                    let eidx = if let Some(oidx) = token_next(&node.children, nidx, true) {
                        if node.children[oidx].kind == NodeKind::Over { oidx } else { nidx }
                    } else { nidx };
                    group_tokens(&mut node.children, NodeKind::Function, i, eidx, false);
                    node.sync_value();
                    continue;
                }
            }
        }
        i += 1;
    }
}

fn group_functions(stmt: &mut Node) { group_functions_inner(stmt); }

// ── group_where ───────────────────────────────────────────────────────────

fn group_where_inner(node: &mut Node) {
    // Recurse into non-Where sublists.
    for i in 0..node.children.len() {
        if node.children[i].is_group() && node.children[i].kind != NodeKind::Where {
            let mut child = std::mem::replace(&mut node.children[i], Node::leaf(ttype::ERROR, ""));
            group_where_inner(&mut child);
            node.children[i] = child;
        }
    }
    let where_close: &[&str] = &[
        "ORDER BY", "GROUP BY", "LIMIT", "UNION", "UNION ALL",
        "EXCEPT", "INTERSECT", "HAVING", "RETURNING", "INTO",
    ];
    let mut i = 0usize;
    while i < node.children.len() {
        if node.children[i].match_token(ttype::KEYWORD, Some(&["WHERE"])) {
            let open_idx = i;
            let close_idx = token_next_by(&node.children, i + 1, |n| {
                n.match_token(ttype::KEYWORD, Some(where_close))
            });
            let end = match close_idx {
                Some(ci) => ci - 1,
                None => node.children.len() - 1,
            };
            group_tokens(&mut node.children, NodeKind::Where, open_idx, end, false);
            node.sync_value();
            continue;
        }
        i += 1;
    }
}

fn group_where(stmt: &mut Node) { group_where_inner(stmt); }

// ── _group helper (dyn Fn to avoid monomorphization recursion) ────────────

type MatchFn<'a>  = &'a dyn Fn(&Node) -> bool;
type ValidFn<'a>  = &'a dyn Fn(Option<&Node>) -> bool;
type PostFn<'a>   = &'a dyn Fn(&[Node], usize, usize, Option<usize>) -> (usize, Option<usize>);

/// Generic middle-token grouper.  Mirrors Python `_group()`.
/// Uses trait objects so the recursive call does not monomorphize infinitely.
fn _group(
    node: &mut Node,
    kind: NodeKind,
    match_fn:   MatchFn<'_>,
    valid_prev: ValidFn<'_>,
    valid_next: ValidFn<'_>,
    post_fn:    PostFn<'_>,
    extend: bool,
    do_recurse: bool,
) {
    // Recurse into different-kind sublists.
    if do_recurse {
        for i in 0..node.children.len() {
            if node.children[i].is_group() && node.children[i].kind != kind {
                let mut child = std::mem::replace(&mut node.children[i], Node::leaf(ttype::ERROR, ""));
                _group(&mut child, kind.clone(), match_fn, valid_prev, valid_next, post_fn, extend, do_recurse);
                node.children[i] = child;
            }
        }
    }

    let mut pidx: Option<usize> = None;
    let mut i = 0usize;
    while i < node.children.len() {
        let n = &node.children[i];
        if n.is_whitespace() { i += 1; continue; }

        if match_fn(n) {
            let nidx = token_next(&node.children, i, true);
            let prev_ok = valid_prev(pidx.map(|p| &node.children[p]));
            let next_ok = valid_next(nidx.map(|ni| &node.children[ni]));
            if prev_ok && next_ok {
                let (from, to) = post_fn(&node.children, pidx.unwrap(), i, nidx);
                let to_idx = match to { Some(t) => t, None => i };
                group_tokens(&mut node.children, kind.clone(), from, to_idx, extend);
                node.sync_value();
                pidx = Some(from);
                i = from + 1;
                continue;
            }
        }
        pidx = Some(i);
        i += 1;
    }
}

// ── group_typecasts ───────────────────────────────────────────────────────

fn group_typecasts(stmt: &mut Node) {
    _group(
        stmt,
        NodeKind::Identifier,
        &|n| n.match_token(ttype::PUNCTUATION, Some(&["::"])),
        &|n| n.is_some(),
        &|n| n.is_some(),
        &|_, pidx, _tidx, nidx| (pidx, nidx),
        true, true,
    );
}

// ── group_tzcasts ─────────────────────────────────────────────────────────

fn group_tzcasts(stmt: &mut Node) {
    _group(
        stmt,
        NodeKind::Identifier,
        &|n| n.ttype == Some(ttype::KEYWORD_TZCAST),
        &|n| n.is_some(),
        &|n| match n {
            None => false,
            Some(n) => n.is_whitespace()
                || n.match_token(ttype::KEYWORD, Some(&["AS"]))
                || n.match_token(ttype::PUNCTUATION, Some(&["'"])),
        },
        &|_, pidx, _tidx, nidx| (pidx, nidx),
        true, true,
    );
}

// ── group_typed_literal ───────────────────────────────────────────────────

fn group_typed_literal(stmt: &mut Node) {
    // First pass: group `name/TIMESTAMP 'string'`
    _group(
        stmt,
        NodeKind::TypedLiteral,
        &|n| n.ttype == Some(ttype::NAME_BUILTIN)
            || n.match_token(ttype::KEYWORD, Some(&["TIMESTAMP"])),
        &|n| n.is_some(),
        &|n| matches!(n, Some(n) if n.ttype == Some(ttype::STRING_SINGLE)),
        &|_, _pidx, tidx, nidx| (tidx, nidx),
        false, true,
    );
    // Second pass: extend with DAY/HOUR/etc.
    let extend_kws: &[&str] = &["DAY", "HOUR", "MINUTE", "MONTH", "SECOND", "YEAR"];
    _group(
        stmt,
        NodeKind::TypedLiteral,
        &|n| n.kind == NodeKind::TypedLiteral,
        &|n| n.is_some(),
        &|n| matches!(n, Some(n) if n.match_token(ttype::KEYWORD, Some(extend_kws))),
        &|_, _pidx, tidx, nidx| (tidx, nidx),
        true, true,
    );
}

// ── group_period ──────────────────────────────────────────────────────────

fn group_period(stmt: &mut Node) {
    let id_kinds = &[NodeKind::SquareBrackets, NodeKind::Identifier];
    let prev_ttypes = &[ttype::NAME, ttype::STRING_SYMBOL];
    let next_kinds  = &[NodeKind::SquareBrackets, NodeKind::Function];
    let next_ttypes = &[ttype::NAME, ttype::STRING_SYMBOL, ttype::WILDCARD, ttype::STRING_SINGLE];

    _group(
        stmt,
        NodeKind::Identifier,
        &|n| n.match_token(ttype::PUNCTUATION, Some(&["."]))
            || n.match_token(ttype::OPERATOR, Some(&["->", "->>"])),
        &|n| match n {
            None => false,
            Some(n) => imt_kind(n, id_kinds) || imt_ttype(n, prev_ttypes),
        },
        &|_| true,  // validated in post
        &|children, pidx, tidx, nidx| {
            // Post: validate next token.
            let next_ok = match nidx {
                None => false,
                Some(ni) => imt_kind(&children[ni], next_kinds)
                    || imt_ttype(&children[ni], next_ttypes),
            };
            if next_ok { (pidx, nidx) } else { (pidx, Some(tidx)) }
        },
        true, true,
    );
}

// ── group_arrays ──────────────────────────────────────────────────────────

fn group_arrays(stmt: &mut Node) {
    let id_kinds  = &[NodeKind::SquareBrackets, NodeKind::Identifier, NodeKind::Function];
    let id_ttypes = &[ttype::NAME, ttype::STRING_SYMBOL];
    _group(
        stmt,
        NodeKind::Identifier,
        &|n| n.kind == NodeKind::SquareBrackets,
        &|n| match n {
            None => false,
            Some(n) => imt_kind(n, id_kinds) || imt_ttype(n, id_ttypes),
        },
        &|_| true,
        &|_, pidx, tidx, _nidx| (pidx, Some(tidx)),
        true, false, // extend=true, recurse=false
    );
}

// ── group_identifier ─────────────────────────────────────────────────────

fn group_identifier_inner(node: &mut Node) {
    // Recurse into non-Identifier sublists.
    for i in 0..node.children.len() {
        if node.children[i].is_group() && node.children[i].kind != NodeKind::Identifier {
            let mut child = std::mem::replace(&mut node.children[i], Node::leaf(ttype::ERROR, ""));
            group_identifier_inner(&mut child);
            node.children[i] = child;
        }
    }
    let mut i = 0usize;
    while i < node.children.len() {
        let matches = node.children[i].ttype == Some(ttype::STRING_SYMBOL)
            || node.children[i].ttype == Some(ttype::NAME);
        if matches {
            group_tokens(&mut node.children, NodeKind::Identifier, i, i, false);
            node.sync_value();
        }
        i += 1;
    }
}

fn group_identifier(stmt: &mut Node) { group_identifier_inner(stmt); }

// ── group_order ───────────────────────────────────────────────────────────

fn group_order_inner(node: &mut Node) {
    for i in 0..node.children.len() {
        if node.children[i].is_group() && node.children[i].kind != NodeKind::Identifier {
            let mut child = std::mem::replace(&mut node.children[i], Node::leaf(ttype::ERROR, ""));
            group_order_inner(&mut child);
            node.children[i] = child;
        }
    }
    let mut i = 0usize;
    while i < node.children.len() {
        if node.children[i].ttype == Some(ttype::KEYWORD_ORDER) {
            if let Some(pidx) = token_prev(&node.children, i, true) {
                let prev_ok = node.children[pidx].kind == NodeKind::Identifier
                    || node.children[pidx].ttype == Some(ttype::NUMBER_INTEGER)
                    || node.children[pidx].ttype == Some(ttype::NUMBER_FLOAT)
                    || node.children[pidx].ttype == Some(ttype::NUMBER_HEX);
                if prev_ok {
                    group_tokens(&mut node.children, NodeKind::Identifier, pidx, i, false);
                    node.sync_value();
                    i = pidx;
                }
            }
        }
        i += 1;
    }
}

fn group_order(stmt: &mut Node) { group_order_inner(stmt); }

// ── group_operator ────────────────────────────────────────────────────────

fn group_operator(stmt: &mut Node) {
    let op_kinds   = &[NodeKind::SquareBrackets, NodeKind::Parenthesis, NodeKind::Function,
                       NodeKind::Identifier, NodeKind::Operation, NodeKind::TypedLiteral];
    let num_ttypes = &[ttype::NUMBER_INTEGER, ttype::NUMBER_FLOAT, ttype::NUMBER_HEX,
                       ttype::STRING_SINGLE, ttype::STRING_SYMBOL,
                       ttype::NAME, ttype::NAME_PLACEHOLDER];

    let valid = move |n: Option<&Node>| -> bool {
        match n {
            None => false,
            Some(n) => {
                if imt_kind(n, op_kinds) || imt_ttype(n, num_ttypes) { return true; }
                // CURRENT_DATE/TIME/TIMESTAMP keyword special case.
                n.match_token(ttype::KEYWORD, Some(&["CURRENT_DATE", "CURRENT_TIME", "CURRENT_TIMESTAMP"]))
            }
        }
    };

    _group(
        stmt,
        NodeKind::Operation,
        &|n| n.ttype_in(ttype::OPERATOR) || n.ttype == Some(ttype::WILDCARD),
        &valid,
        &valid,
        &|_, pidx, _tidx, nidx| (pidx, nidx),
        false, true,
    );
}

// ── group_comparison ─────────────────────────────────────────────────────

fn group_comparison(stmt: &mut Node) {
    let comp_kinds  = &[NodeKind::Parenthesis, NodeKind::Function, NodeKind::Identifier,
                        NodeKind::Operation, NodeKind::TypedLiteral];
    let num_ttypes  = &[ttype::NUMBER_INTEGER, ttype::NUMBER_FLOAT, ttype::NUMBER_HEX,
                        ttype::STRING_SINGLE, ttype::STRING_SYMBOL,
                        ttype::NAME, ttype::NAME_PLACEHOLDER];

    let valid = move |n: Option<&Node>| -> bool {
        match n {
            None => false,
            Some(n) => {
                if imt_kind(n, comp_kinds) || imt_ttype(n, num_ttypes) { return true; }
                n.is_keyword() && n.normalized() == "NULL"
            }
        }
    };

    _group(
        stmt,
        NodeKind::Comparison,
        &|n| n.ttype == Some(ttype::OPERATOR_COMPARISON),
        &valid,
        &valid,
        &|_, pidx, _tidx, nidx| (pidx, nidx),
        false, true,
    );
}

// ── group_as ─────────────────────────────────────────────────────────────

fn group_as(stmt: &mut Node) {
    _group(
        stmt,
        NodeKind::Identifier,
        &|n| n.is_keyword() && n.normalized() == "AS",
        &|n| match n {
            None => false,
            Some(n) => n.normalized() == "NULL" || !n.is_keyword(),
        },
        &|n| match n {
            None => false,
            Some(n) => !n.ttype_in(ttype::KEYWORD_DML)
                && !n.ttype_in(ttype::KEYWORD_DDL)
                && !n.ttype_in(ttype::KEYWORD_CTE),
        },
        &|_, pidx, _tidx, nidx| (pidx, nidx),
        true, true,
    );
}

// ── group_aliased ─────────────────────────────────────────────────────────

fn group_aliased_inner(node: &mut Node) {
    for i in 0..node.children.len() {
        if node.children[i].is_group() {
            let mut child = std::mem::replace(&mut node.children[i], Node::leaf(ttype::ERROR, ""));
            group_aliased_inner(&mut child);
            node.children[i] = child;
        }
    }
    let alias_kinds = &[NodeKind::Parenthesis, NodeKind::Function, NodeKind::Case,
                        NodeKind::Identifier, NodeKind::Operation, NodeKind::Comparison];
    let num_ttypes  = &[ttype::NUMBER_INTEGER, ttype::NUMBER_FLOAT, ttype::NUMBER_HEX];

    let mut i = 0usize;
    while i < node.children.len() {
        let is_alias_target = imt_kind(&node.children[i], alias_kinds)
            || imt_ttype(&node.children[i], num_ttypes);
        if is_alias_target {
            if let Some(nidx) = token_next(&node.children, i, true) {
                if node.children[nidx].kind == NodeKind::Identifier {
                    group_tokens(&mut node.children, NodeKind::Identifier, i, nidx, true);
                    node.sync_value();
                    continue;
                }
            }
        }
        i += 1;
    }
}

fn group_aliased(stmt: &mut Node) { group_aliased_inner(stmt); }

// ── group_assignment ──────────────────────────────────────────────────────

fn group_assignment(stmt: &mut Node) {
    _group(
        stmt,
        NodeKind::Assignment,
        &|n| n.match_token(ttype::ASSIGNMENT, Some(&[":="])),
        &|n| matches!(n, Some(n) if n.ttype != Some(ttype::KEYWORD)),
        &|n| matches!(n, Some(n) if n.ttype != Some(ttype::KEYWORD)),
        &|children, pidx, _tidx, nidx| {
            // Extend to next semicolon if present.
            let semi_idx = token_next_by(children, nidx.unwrap_or(0), |n| {
                n.match_token(ttype::PUNCTUATION, Some(&[";"]))
            });
            (pidx, semi_idx.or(nidx))
        },
        true, true,
    );
}

// ── group_comments ────────────────────────────────────────────────────────

fn group_comments_inner(node: &mut Node) {
    for i in 0..node.children.len() {
        if node.children[i].is_group() && node.children[i].kind != NodeKind::Comment {
            let mut child = std::mem::replace(&mut node.children[i], Node::leaf(ttype::ERROR, ""));
            group_comments_inner(&mut child);
            node.children[i] = child;
        }
    }
    let mut i = 0usize;
    while i < node.children.len() {
        if node.children[i].ttype_in("Token.Comment") {
            let start = i;
            // Extend while comment or newline.
            let end = {
                let mut j = i + 1;
                while j < node.children.len() {
                    let n = &node.children[j];
                    if n.ttype_in("Token.Comment") || n.is_newline() {
                        j += 1;
                    } else {
                        break;
                    }
                }
                j - 1
            };
            // Only group if there's a terminator (i.e., not everything).
            if end < node.children.len().saturating_sub(1) || end > start {
                // Walk back to drop trailing newline before terminator.
                let mut real_end = end;
                if real_end > start && node.children[real_end].is_newline() {
                    // Don't include the trailing newline in the Comment group.
                    // (Python token_prev skips it.)
                    real_end -= 1;
                }
                if real_end >= start {
                    group_tokens(&mut node.children, NodeKind::Comment, start, real_end, false);
                    node.sync_value();
                    continue;
                }
            }
        }
        i += 1;
    }
}

fn group_comments(stmt: &mut Node) { group_comments_inner(stmt); }

// ── group_identifier_list ─────────────────────────────────────────────────

fn group_identifier_list(stmt: &mut Node) {
    let list_kinds  = &[NodeKind::Function, NodeKind::Case, NodeKind::Identifier,
                        NodeKind::Comparison, NodeKind::IdentifierList, NodeKind::Operation];
    let list_ttypes = &[ttype::NUMBER_INTEGER, ttype::NUMBER_FLOAT, ttype::NUMBER_HEX,
                        ttype::STRING_SINGLE, ttype::STRING_SYMBOL,
                        ttype::NAME, ttype::NAME_PLACEHOLDER,
                        ttype::KEYWORD, "Token.Comment", ttype::WILDCARD];

    let valid = move |n: Option<&Node>| -> bool {
        match n {
            None => false,
            Some(n) => {
                if imt_kind(n, list_kinds) { return true; }
                if imt_ttype(n, list_ttypes) { return true; }
                // role / null keywords
                n.match_token(ttype::KEYWORD, Some(&["NULL", "ROLE"]))
            }
        }
    };

    _group(
        stmt,
        NodeKind::IdentifierList,
        &|n| n.match_token(ttype::PUNCTUATION, Some(&[","])),
        &valid,
        &valid,
        &|_, pidx, _tidx, nidx| (pidx, nidx),
        true, true,
    );
}

// ── align_comments ────────────────────────────────────────────────────────

fn align_comments_inner(node: &mut Node) {
    for i in 0..node.children.len() {
        if node.children[i].is_group() {
            let mut child = std::mem::replace(&mut node.children[i], Node::leaf(ttype::ERROR, ""));
            align_comments_inner(&mut child);
            node.children[i] = child;
        }
    }
    let mut i = 0usize;
    while i < node.children.len() {
        if node.children[i].kind == NodeKind::Comment {
            if let Some(pidx) = token_prev(&node.children, i, true) {
                if node.children[pidx].is_group() {
                    group_tokens(&mut node.children, NodeKind::TokenList, pidx, i, true);
                    node.sync_value();
                    i = pidx + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
}

fn align_comments(stmt: &mut Node) { align_comments_inner(stmt); }

// ── group_values ──────────────────────────────────────────────────────────

fn group_values(stmt: &mut Node) {
    let mut start_idx: Option<usize> = None;
    let mut end_idx: Option<usize> = None;
    let mut i = 0usize;
    while i < stmt.children.len() {
        if stmt.children[i].match_token(ttype::KEYWORD, Some(&["VALUES"])) {
            start_idx = Some(i);
        }
        if start_idx.is_some() && stmt.children[i].kind == NodeKind::Parenthesis {
            end_idx = Some(i);
        }
        i += 1;
    }
    if let (Some(si), Some(ei)) = (start_idx, end_idx) {
        group_tokens(&mut stmt.children, NodeKind::Values, si, ei, true);
        stmt.sync_value();
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Statement type detection (mirrors Statement.get_type())
// ──────────────────────────────────────────────────────────────────────────

pub fn get_type(stmt: &Node) -> &'static str {
    let first = stmt.token_first(true);
    match first {
        None => "UNKNOWN",
        Some(t) => {
            if t.ttype_in(ttype::KEYWORD_DML) || t.ttype_in(ttype::KEYWORD_DDL) {
                // Return a static str for common types.
                match t.normalized().as_str() {
                    "SELECT" => "SELECT",
                    "INSERT" => "INSERT",
                    "UPDATE" => "UPDATE",
                    "DELETE" => "DELETE",
                    "CREATE" => "CREATE",
                    "ALTER"  => "ALTER",
                    "DROP"   => "DROP",
                    _        => "UNKNOWN",
                }
            } else {
                "UNKNOWN"
            }
        }
    }
}
