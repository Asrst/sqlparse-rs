// src/lexer.rs — Rust SQL lexer
// Produces a flat Vec<(TType, String)> stream from a SQL string.

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;

use crate::keywords::{
    self, TType, PROCESS_AS_KEYWORD,
    T_ERROR, T_COMMENT_MULTI, T_COMMENT_SINGLE, T_STRING_SINGLE,
    T_STRING_SYMBOL, T_NEWLINE,
};

pub type Token = (TType, String);

// Pre-compiled regex rules
struct Rule {
    re: Regex,
    ttype: TType,
}

static RULES: Lazy<Vec<Rule>> = Lazy::new(|| {
    keywords::sql_regex_rules()
        .into_iter()
        .map(|(pat, tt)| Rule {
            re: Regex::new(pat).expect("bad regex"),
            ttype: tt,
        })
        .collect()
});

static KEYWORD_MAP: Lazy<HashMap<&'static str, TType>> =
    Lazy::new(keywords::build_keyword_map);

pub fn tokenize_str(text: &str) -> Vec<Token> {
    let mut tokens: Vec<Token> = Vec::with_capacity(text.len() / 4);
    let mut pos = 0;
    let bytes = text.as_bytes();
    let len = text.len();

    while pos < len {
        let slice = &text[pos..];

        // Fast path: handle dollar-quoted strings ($$...$$) and ($tag$...$tag$)
        if bytes[pos] == b'$' {
            if let Some((end, val)) = match_dollar_quoted(text, pos) {
                tokens.push((crate::keywords::T_STRING_SINGLE, val));
                pos = end;
                continue;
            }
        }

        let mut matched = false;
        for rule in RULES.iter() {
            if let Some(m) = rule.re.find(slice) {
                if m.start() != 0 {
                    continue;
                }
                let val = m.as_str().to_string();
                let tt = if rule.ttype == PROCESS_AS_KEYWORD {
                    let upper = val.to_uppercase();
                    *KEYWORD_MAP.get(upper.as_str()).unwrap_or(&crate::keywords::T_NAME)
                } else {
                    rule.ttype
                };
                tokens.push((tt, val.clone()));
                pos += val.len();
                matched = true;
                break;
            }
        }

        if !matched {
            // Emit one error character
            let ch = &text[pos..pos + text[pos..].chars().next().map(|c| c.len_utf8()).unwrap_or(1)];
            tokens.push((T_ERROR, ch.to_string()));
            pos += ch.len();
        }
    }

    tokens
}

/// Match a dollar-quoted string starting at `pos`.
/// Returns (end_pos, matched_string) or None.
fn match_dollar_quoted(text: &str, pos: usize) -> Option<(usize, String)> {
    let rest = &text[pos..];
    // Find the closing $tag$
    let tag_end = rest[1..].find('$')? + 2; // position after second $
    let tag = &rest[..tag_end]; // e.g. "$$" or "$func$"

    // Only allow alphanumeric/underscore tags
    if !rest[1..tag_end - 1].chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }

    // Find matching close tag
    let body_start = tag_end;
    let close = rest[body_start..].find(tag)?;
    let end_in_rest = body_start + close + tag.len();
    let matched = rest[..end_in_rest].to_string();
    Some((pos + end_in_rest, matched))
}
