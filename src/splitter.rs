// src/splitter.rs — Statement splitter
// Ported from sqlparse/engine/statement_splitter.py

use crate::keywords::{
    TType, T_KEYWORD, T_KEYWORD_DDL, T_PUNCTUATION, T_WHITESPACE, T_NEWLINE,
    T_COMMENT_SINGLE, T_COMMENT_MULTI,
};
use crate::lexer::{tokenize_str, Token};

struct SplitState {
    block_stack: Vec<&'static str>,
    parenthesis_level: i32,
    unconfirmed_start: Option<&'static str>,
    is_create: bool,
    seen_begin: bool,
    level: i32,
    consume_ws: bool,
}

impl SplitState {
    fn new() -> Self {
        SplitState {
            block_stack: Vec::new(),
            parenthesis_level: 0,
            unconfirmed_start: None,
            is_create: false,
            seen_begin: false,
            level: 0,
            consume_ws: false,
        }
    }

    fn reset(&mut self) {
        self.block_stack.clear();
        self.parenthesis_level = 0;
        self.unconfirmed_start = None;
        self.is_create = false;
        self.seen_begin = false;
        self.level = 0;
        self.consume_ws = false;
    }

    fn change_split_level(&mut self, ttype: TType, value: &str) -> i32 {
        let unified = value.to_uppercase();

        if ttype == T_PUNCTUATION && value == ";" {
            self.unconfirmed_start = None;
            if self.seen_begin {
                self.seen_begin = false;
                if self.block_stack.last() == Some(&"BEGIN") {
                    self.block_stack.pop();
                    return -1;
                }
            }
            return 0;
        }

        if ttype == T_PUNCTUATION && value == "(" {
            self.parenthesis_level += 1;
            return 1;
        }
        if ttype == T_PUNCTUATION && value == ")" {
            self.parenthesis_level = (self.parenthesis_level - 1).max(0);
            return -1;
        }

        if ttype != T_KEYWORD && ttype != T_KEYWORD_DDL {
            return 0;
        }

        // DDL CREATE
        if ttype == T_KEYWORD_DDL && unified.starts_with("CREATE") {
            self.is_create = true;
            return 0;
        }

        if unified == "DECLARE" && self.is_create && self.block_stack.is_empty() {
            self.block_stack.push("DECLARE");
            return 1;
        }

        if unified == "BEGIN" {
            self.seen_begin = true;
            if self.block_stack.last() == Some(&"DECLARE") {
                self.block_stack.pop();
                self.block_stack.push("BEGIN");
                return 0;
            }
            self.block_stack.push("BEGIN");
            return 1;
        }

        // Transaction after BEGIN
        if self.seen_begin {
            let txn_kw = matches!(
                unified.as_str(),
                "TRANSACTION" | "WORK" | "TRAN" | "DISTRIBUTED" |
                "DEFERRED" | "IMMEDIATE" | "EXCLUSIVE"
            );
            if txn_kw {
                self.seen_begin = false;
                if self.block_stack.last() == Some(&"BEGIN") {
                    self.block_stack.pop();
                    return -1;
                }
                return 0;
            }
        }

        // Nested blocks inside BEGIN
        if self.block_stack.contains(&"BEGIN") {
            match unified.as_str() {
                "FOR" => { self.unconfirmed_start = Some("FOR"); return 0; }
                "WHILE" => { self.unconfirmed_start = Some("WHILE"); return 0; }
                "LOOP" | "DO" => {
                    if self.unconfirmed_start == Some("FOR") || self.unconfirmed_start == Some("WHILE") {
                        let tag = self.unconfirmed_start.unwrap();
                        self.block_stack.push(tag);
                        self.unconfirmed_start = None;
                        return 1;
                    }
                    if unified == "LOOP" {
                        self.block_stack.push("LOOP");
                        return 1;
                    }
                }
                "IF" | "CASE" => {
                    self.block_stack.push(if unified == "IF" { "IF" } else { "CASE" });
                    return 1;
                }
                _ => {}
            }
        }

        // Closing keywords
        match unified.as_str() {
            "END IF" => {
                if self.block_stack.last() == Some(&"IF") { self.block_stack.pop(); return -1; }
            }
            "END FOR" => {
                if self.block_stack.last() == Some(&"FOR") { self.block_stack.pop(); return -1; }
            }
            "END WHILE" => {
                if self.block_stack.last() == Some(&"WHILE") { self.block_stack.pop(); return -1; }
            }
            "END LOOP" => {
                if matches!(self.block_stack.last(), Some(&"LOOP") | Some(&"FOR") | Some(&"WHILE")) {
                    self.block_stack.pop(); return -1;
                }
            }
            "END CASE" => {
                if self.block_stack.last() == Some(&"CASE") { self.block_stack.pop(); return -1; }
            }
            "END" => {
                if !self.block_stack.is_empty() { self.block_stack.pop(); }
                return -1;
            }
            _ => {}
        }

        0
    }
}

/// Returns a Vec of raw SQL statement strings (with whitespace preserved).
pub fn split_statements(sql: &str, strip_semicolon: bool) -> Vec<String> {
    let tokens = tokenize_str(sql);
    let eos_types = [T_WHITESPACE, T_NEWLINE, T_COMMENT_SINGLE];

    let mut state = SplitState::new();
    let mut current: Vec<String> = Vec::new();
    let mut results: Vec<String> = Vec::new();

    for (ttype, value) in &tokens {
        // If we finished a statement and hit a non-EOS token, flush
        if state.consume_ws && !eos_types.contains(ttype) {
            let stmt = current.concat();
            let trimmed = stmt.trim().to_string();
            if !trimmed.is_empty() {
                results.push(trimmed);
            }
            current.clear();
            state.reset();
        }

        state.level += state.change_split_level(ttype, value);
        current.push(value.clone());

        if *ttype == T_PUNCTUATION && value == ";" {
            state.seen_begin = false;
            if state.level <= 0 && !state.block_stack.contains(&"BEGIN") {
                state.consume_ws = true;
            }
        } else if *ttype == crate::keywords::T_KEYWORD
            && value.split_whitespace().next().map(|w| w.eq_ignore_ascii_case("GO")).unwrap_or(false)
        {
            state.consume_ws = true;
        }
    }

    // Flush remaining
    let stmt = current.concat();
    let mut trimmed = stmt.trim().to_string();
    if !trimmed.is_empty() {
        if strip_semicolon {
            trimmed = trimmed.trim_end_matches(';').trim().to_string();
        }
        results.push(trimmed);
    }

    if strip_semicolon {
        results = results
            .into_iter()
            .map(|s| s.trim_end_matches(';').trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }

    results
}
