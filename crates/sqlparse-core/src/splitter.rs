//! Tier 2 — 1:1 port of `engine/statement_splitter.py` (sqlparse 0.6.0).
//!
//! Consumes the token stream produced by [`crate::lexer::tokenize`] and
//! yields statements as joined strings (equivalent to `sqlparse.split()`).
//! All quirks are replicated:
//! * `;` splits only at level <= 0 and outside BEGIN...END blocks
//! * standalone `BEGIN;` splits (the semicolon pops the block)
//! * `BEGIN TRANSACTION`/`WORK`/... undo the block (issue 826)
//! * DECLARE -> BEGIN transition, FOR/WHILE staging + LOOP/DO confirmation,
//!   END IF/END FOR/END WHILE/END LOOP/END CASE closers matched against the
//!   stack top, bare END pops anything
//! * GO / GO 2 splitter (issue 762); `_seen_begin` reset rules
//! * EOS_TTYPE = (Whitespace, Comment.Single) by equality — Newline is NOT
//!   EOS; final flush drops all-whitespace tails

use crate::lexer;
use crate::ttype;

pub fn split(input: &str) -> Vec<String> {
    let tokens = lexer::tokenize(input);
    let mut out: Vec<String> = Vec::new();
    let mut st = Splitter::default();

    for &(ttype, value) in &tokens {
        if st.consume_ws && !ttype::is_eos(ttype) {
            out.push(st.take());
        }
        st.level += st.change_splitlevel(ttype, value);
        st.tokens.push((ttype, value.to_string()));

        if ttype == ttype::PUNCTUATION && value == ";" {
            st.seen_begin = false;
            if st.level <= 0 && !st.block_stack.iter().any(|b| b == "BEGIN") {
                st.consume_ws = true;
            }
        } else if ttype == ttype::KEYWORD && first_word(value) == "GO" {
            st.consume_ws = true;
        } else if !matches!(
            ttype,
            ttype::WHITESPACE | ttype::NEWLINE | ttype::COMMENT_SINGLE | ttype::COMMENT_MULTILINE
        ) && !(ttype == ttype::KEYWORD && value.to_uppercase() == "BEGIN")
        {
            st.seen_begin = false;
        }
    }

    if !st.tokens.is_empty() && !st.tokens.iter().all(|(t, _)| ttype::is_ws_like(t)) {
        out.push(st.take());
    }
    out
}

fn first_word(value: &str) -> &str {
    value.split_whitespace().next().unwrap_or("")
}

/// Token-level statement splitting — what python's `StatementSplitter`
/// actually yields to the formatter: raw token lists with NO stripping, so
/// trailing whitespace/comment tokens stay inside their statement (e.g. the
/// newline stored inside a trailing comment value must survive into the
/// formatted output). The string-stripping behaviour is exclusive to the
/// `split()` API (`str(stmt).strip()`).
pub fn split_tokens(input: &str) -> Vec<Vec<(&'static str, String)>> {
    let tokens = lexer::tokenize(input);
    let mut out: Vec<Vec<(&'static str, String)>> = Vec::new();
    let mut st = Splitter::default();

    for &(ttype, value) in &tokens {
        if st.consume_ws && !ttype::is_eos(ttype) {
            let taken = std::mem::take(&mut st.tokens);
            if !taken.iter().all(|(t, _)| ttype::is_ws_like(t)) {
                out.push(taken);
            }
            st.reset();
        }
        st.level += st.change_splitlevel(ttype, value);
        st.tokens.push((ttype, value.to_string()));

        if ttype == ttype::PUNCTUATION && value == ";" {
            st.seen_begin = false;
            if st.level <= 0 && !st.block_stack.iter().any(|b| b == "BEGIN") {
                st.consume_ws = true;
            }
        } else if ttype == ttype::KEYWORD && first_word(value) == "GO" {
            st.consume_ws = true;
        } else if !matches!(
            ttype,
            ttype::WHITESPACE | ttype::NEWLINE | ttype::COMMENT_SINGLE | ttype::COMMENT_MULTILINE
        ) && !(ttype == ttype::KEYWORD && value.to_uppercase() == "BEGIN")
        {
            st.seen_begin = false;
        }
    }

    if !st.tokens.is_empty() && !st.tokens.iter().all(|(t, _)| ttype::is_ws_like(t)) {
        let taken = std::mem::take(&mut st.tokens);
        if !taken.iter().all(|(t, _)| ttype::is_ws_like(t)) {
            out.push(taken);
        }
    }
    out
}

#[derive(Default)]
struct Splitter {
    block_stack: Vec<String>,
    parenthesis_level: i32,
    unconfirmed_start: Option<String>,
    is_create: bool,
    seen_begin: bool,
    consume_ws: bool,
    tokens: Vec<(&'static str, String)>,
    level: i32,
}

impl Splitter {
    /// str(stmt).strip() — Python's split() strips every statement
    /// (this is where inter-statement whitespace disappears).
    fn take(&mut self) -> String {
        let s: String = self.tokens.iter().map(|(_, v)| v.as_str()).collect();
        self.reset();
        s.trim().to_string()
    }

    fn reset(&mut self) {
        self.block_stack.clear();
        self.parenthesis_level = 0;
        self.unconfirmed_start = None;
        self.is_create = false;
        self.seen_begin = false;
        self.consume_ws = false;
        self.tokens.clear();
        self.level = 0;
    }

    fn change_splitlevel(&mut self, ttype: &str, value: &str) -> i32 {
        // Semicolon resets unconfirmed loop starters; handles standalone BEGIN;
        if ttype == ttype::PUNCTUATION && value == ";" {
            self.unconfirmed_start = None;
            if self.seen_begin {
                self.seen_begin = false;
                if self.block_stack.last().map(String::as_str) == Some("BEGIN") {
                    self.block_stack.pop();
                    return -1;
                }
            }
            return 0;
        }

        // parenthesis increase/decrease a level
        if ttype == ttype::PUNCTUATION && value == "(" {
            self.parenthesis_level += 1;
            return 1;
        } else if ttype == ttype::PUNCTUATION && value == ")" {
            self.parenthesis_level = (self.parenthesis_level - 1).max(0);
            return -1;
        } else if !ttype::is_keyword_family(ttype) {
            // hierarchical `ttype not in T.Keyword`
            return 0;
        }

        let unified = value.to_uppercase();

        if ttype == ttype::KEYWORD_DDL && unified.starts_with("CREATE") {
            self.is_create = true;
            return 0;
        }

        if unified == "DECLARE" && self.is_create && self.block_stack.is_empty() {
            self.block_stack.push("DECLARE".to_string());
            return 1;
        }

        if unified == "BEGIN" {
            self.seen_begin = true;
            if self.block_stack.last().map(String::as_str) == Some("DECLARE") {
                self.block_stack.pop();
                self.block_stack.push("BEGIN".to_string());
                return 0;
            } else {
                self.block_stack.push("BEGIN".to_string());
                return 1;
            }
        }

        // Issue826: transaction keyword after BEGIN -> not a block
        if self.seen_begin
            && (ttype == ttype::KEYWORD || ttype == ttype::NAME)
            && matches!(
                unified.as_str(),
                "TRANSACTION" | "WORK" | "TRAN" | "DISTRIBUTED" | "DEFERRED" | "IMMEDIATE"
                    | "EXCLUSIVE"
            )
        {
            self.seen_begin = false;
            if self.block_stack.last().map(String::as_str) == Some("BEGIN") {
                self.block_stack.pop();
                return -1;
            }
            return 0;
        }

        if self.block_stack.iter().any(|b| b == "BEGIN") {
            if let Some(r) = self.handle_nested_block(&unified) {
                return r;
            }
        }

        self.handle_closing_keyword(&unified)
    }

    fn handle_nested_block(&mut self, unified: &str) -> Option<i32> {
        if unified == "FOR" || unified == "WHILE" {
            self.unconfirmed_start = Some(unified.to_string());
            return Some(0);
        }
        if unified == "LOOP" || unified == "DO" {
            let confirmed = matches!(
                self.unconfirmed_start.as_deref(),
                Some("FOR") | Some("WHILE")
            );
            if confirmed {
                let u = self.unconfirmed_start.take().unwrap();
                self.block_stack.push(u);
                return Some(1);
            }
            if unified == "LOOP" {
                self.block_stack.push("LOOP".to_string());
                return Some(1);
            }
        }
        if unified == "IF" || unified == "CASE" {
            self.block_stack.push(unified.to_string());
            return Some(1);
        }
        None
    }

    fn handle_closing_keyword(&mut self, unified: &str) -> i32 {
        let last_is = |s: &Splitter, k: &str| s.block_stack.last().map(String::as_str) == Some(k);
        if unified == "END IF" {
            if last_is(self, "IF") {
                self.block_stack.pop();
                return -1;
            }
        } else if unified == "END FOR" {
            if last_is(self, "FOR") {
                self.block_stack.pop();
                return -1;
            }
        } else if unified == "END WHILE" {
            if last_is(self, "WHILE") {
                self.block_stack.pop();
                return -1;
            }
        } else if unified == "END LOOP" {
            if self
                .block_stack
                .last()
                .map(String::as_str)
                .map(|k| matches!(k, "LOOP" | "FOR" | "WHILE"))
                .unwrap_or(false)
            {
                self.block_stack.pop();
                return -1;
            }
        } else if unified == "END CASE" {
            if last_is(self, "CASE") {
                self.block_stack.pop();
                return -1;
            }
        } else if unified == "END" {
            if !self.block_stack.is_empty() {
                self.block_stack.pop();
            }
            return -1;
        }
        0
    }
}
