//! Port of `keywords.find_delimited_spans` + `DelimitedSpans` (sqlparse 0.6.0).
//!
//! Single pass collecting opener/closer positions of dollar-quoted literals
//! and multiline comments; the lexer resolves an opener by binary search when
//! it actually reaches that position. Positions are CHAR indices into the
//! scanned text (the lexer works in the same coordinate space).
//!
//! Faithfulness notes (from the Python source):
//! * `_DOLLAR_QUOTE_DELIM` is wrapped in a lookahead so finditer reports
//!   OVERLAPPING occurrences (in `$$$$` the closing `$$` starts inside the
//!   opening one) — we check every `$` position.
//! * Every delimiter occurrence is a potential closer; only occurrences NOT
//!   preceded by a word char, `"` or `$` can open (`_DOLLAR_QUOTE_OPENER_OK`).
//! * Comment openers: `/*+` (hint) checked before `/*` (plain, via `(?!\+)`);
//!   all comments share the `/*` closer bucket.
//! * `resolve` pairs an opener with the FIRST closer whose start is at or
//!   after the opener end; none -> None (the lexer then falls through to the
//!   regex table — an unclosed `$$` does not swallow the rest of the input).

use std::collections::HashMap;

use crate::ttype;

pub struct DelimitedSpans {
    /// start -> (end behind opener, tag, token type)
    openers: HashMap<usize, (usize, String, &'static str)>,
    /// tag -> ascending (start, end) closer occurrences
    closers: HashMap<String, Vec<(usize, usize)>>,
}

impl DelimitedSpans {
    /// (end offset, token type) for the span opening at `pos`, or None.
    pub fn resolve(&self, pos: usize) -> Option<(usize, &'static str)> {
        let (opener_end, tag, ttype) = self.openers.get(&pos)?;
        let occ = self.closers.get(tag)?;
        let idx = occ.partition_point(|(start, _)| *start < *opener_end);
        if idx == occ.len() {
            return None;
        }
        Some((occ[idx].1, ttype))
    }
}

const COMMENT_TAG: &str = "/*";

pub fn find(chars: &[char]) -> DelimitedSpans {
    let has_dollar = chars.contains(&'$');
    let has_comment_open = chars.windows(2).any(|w| w[0] == '/' && w[1] == '*');
    let mut openers: HashMap<usize, (usize, String, &'static str)> = HashMap::new();
    let mut closers: HashMap<String, Vec<(usize, usize)>> = HashMap::new();
    if !has_dollar && !has_comment_open {
        return DelimitedSpans { openers, closers };
    }

    let n = chars.len();
    if has_dollar {
        let mut i = 0;
        while i < n {
            if chars[i] == '$' {
                if let Some((tag_end, tag)) = match_dollar_tag(chars, i) {
                    // Every occurrence is a potential closer.
                    closers.entry(tag.clone()).or_default().push((i, tag_end));
                    // Opener only if NOT preceded by \w, `"` or `$`.
                    let opener_ok = i == 0 || {
                        let p = chars[i - 1];
                        !(is_word_char(p) || p == '"' || p == '$')
                    };
                    if opener_ok {
                        openers.insert(i, (tag_end, tag.clone(), ttype::LITERAL));
                    }
                }
            }
            i += 1;
        }
    }

    if has_comment_open {
        // Python uses THREE independent finditer passes; a closer may
        // overlap an opener (`/*/` — the `*/` starts one char in), so the
        // passes must not skip each other's characters.
        // pass 1: /*+ hint openers
        let mut i = 0usize;
        while i + 2 < n {
            if chars[i] == '/' && chars[i + 1] == '*' && chars[i + 2] == '+' {
                openers.insert(
                    i,
                    (
                        i + 3,
                        COMMENT_TAG.to_string(),
                        ttype::COMMENT_MULTILINE_HINT,
                    ),
                );
                i += 3;
            } else {
                i += 1;
            }
        }
        // pass 2: /* plain openers (not followed by '+')
        let mut i = 0usize;
        while i + 1 < n {
            if chars[i] == '/' && chars[i + 1] == '*' && !(i + 2 < n && chars[i + 2] == '+') {
                openers.insert(
                    i,
                    (i + 2, COMMENT_TAG.to_string(), ttype::COMMENT_MULTILINE),
                );
                i += 2;
            } else {
                i += 1;
            }
        }
        // pass 3: */ closers
        let mut i = 0usize;
        while i + 1 < n {
            if chars[i] == '*' && chars[i + 1] == '/' {
                closers
                    .entry(COMMENT_TAG.to_string())
                    .or_default()
                    .push((i, i + 2));
                i += 2;
            } else {
                i += 1;
            }
        }
    }

    // Occurrences were collected in ascending order; keep them sorted by start.
    for v in closers.values_mut() {
        v.sort_by_key(|(s, _)| *s);
    }
    DelimitedSpans { openers, closers }
}

/// `$` + optional `[_A-ZÀ-Ü]\w*` (IGNORECASE|UNICODE) + `$`.
/// Returns (end index, tag string) e.g. `$$` or `$tag$`.
fn match_dollar_tag(chars: &[char], pos: usize) -> Option<(usize, String)> {
    let n = chars.len();
    if pos >= n || chars[pos] != '$' {
        return None;
    }
    let mut j = pos + 1;
    // `[_A-ZÀ-Ü]` with IGNORECASE: ASCII letters + Latin-1 À-Ü/à-ü range.
    if j < n && is_tag_letter(chars[j]) {
        while j < n && is_word_char(chars[j]) {
            j += 1;
        }
    }
    if j < n && chars[j] == '$' {
        Some((j + 1, chars[pos..=j].iter().collect()))
    } else {
        None
    }
}

fn is_tag_letter(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic() || ('À'..='Ü').contains(&c) || ('à'..='ü').contains(&c)
}

/// Approximation of Python `re` `\w` with re.UNICODE: word chars per
/// `str.isalnum()`-ish plus underscore. (Rust `is_alphanumeric` includes
/// combining marks that Python `\w` excludes — a known, flagged edge.)
pub fn is_word_char(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}
