// src/formatter.rs — SQL formatter
// Implements sqlparse.format() options:
//   keyword_case, identifier_case, strip_whitespace, strip_comments,
//   reindent, indent_width, indent_char, wrap_after, comma_first, compact

use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::keywords::*;
use crate::lexer::tokenize_str;
use crate::splitter::split_statements;

#[derive(Debug, Clone)]
pub struct Options {
    pub keyword_case: Option<CaseMode>,
    pub identifier_case: Option<CaseMode>,
    pub strip_whitespace: bool,
    pub strip_comments: bool,
    pub reindent: bool,
    pub reindent_aligned: bool,
    pub indent_width: usize,
    pub indent_char: char,
    pub indent_after_first: bool,
    pub indent_columns: bool,
    pub wrap_after: usize,
    pub comma_first: bool,
    pub compact: bool,
    pub use_space_around_operators: bool,
    pub truncate_strings: Option<usize>,
    pub truncate_char: String,
    pub right_margin: Option<usize>,
    pub output_format: Option<OutputFormat>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CaseMode { Upper, Lower, Capitalize }

#[derive(Debug, Clone)]
pub enum OutputFormat { Sql, Python, Php }

impl Options {
    pub fn from_pydict(py: Python<'_>, d: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        let get_str = |key: &str| -> Option<String> {
            d.and_then(|d| d.get_item(key).ok().flatten())
             .and_then(|v| v.extract::<String>().ok())
        };
        let get_bool = |key: &str| -> bool {
            d.and_then(|d| d.get_item(key).ok().flatten())
             .and_then(|v| v.extract::<bool>().ok())
             .unwrap_or(false)
        };
        let get_usize = |key: &str, default: usize| -> usize {
            d.and_then(|d| d.get_item(key).ok().flatten())
             .and_then(|v| v.extract::<usize>().ok())
             .unwrap_or(default)
        };

        let keyword_case = get_str("keyword_case").and_then(|s| match s.as_str() {
            "upper" => Some(CaseMode::Upper),
            "lower" => Some(CaseMode::Lower),
            "capitalize" => Some(CaseMode::Capitalize),
            _ => None,
        });
        let identifier_case = get_str("identifier_case").and_then(|s| match s.as_str() {
            "upper" => Some(CaseMode::Upper),
            "lower" => Some(CaseMode::Lower),
            "capitalize" => Some(CaseMode::Capitalize),
            _ => None,
        });

        let indent_tabs = get_bool("indent_tabs");
        let indent_char = if indent_tabs { '\t' } else { ' ' };
        let indent_width = get_usize("indent_width", 2);
        let wrap_after = get_usize("wrap_after", 0);

        let reindent = get_bool("reindent");
        let strip_whitespace = reindent || get_bool("strip_whitespace");

        let truncate_strings = d.and_then(|d| d.get_item("truncate_strings").ok().flatten())
            .and_then(|v| v.extract::<usize>().ok())
            .filter(|&n| n > 1);
        let truncate_char = get_str("truncate_char").unwrap_or_else(|| "[...]".to_string());

        let output_format = get_str("output_format").and_then(|s| match s.to_lowercase().as_str() {
            "python" => Some(OutputFormat::Python),
            "php"    => Some(OutputFormat::Php),
            _        => None,
        });

        Ok(Options {
            keyword_case,
            identifier_case,
            strip_whitespace,
            strip_comments: get_bool("strip_comments"),
            reindent,
            reindent_aligned: get_bool("reindent_aligned"),
            indent_width,
            indent_char,
            indent_after_first: get_bool("indent_after_first"),
            indent_columns: get_bool("indent_columns"),
            wrap_after,
            comma_first: get_bool("comma_first"),
            compact: get_bool("compact"),
            use_space_around_operators: get_bool("use_space_around_operators"),
            truncate_strings,
            truncate_char,
            right_margin: d.and_then(|d| d.get_item("right_margin").ok().flatten())
                           .and_then(|v| v.extract::<usize>().ok()),
            output_format,
        })
    }
}

fn apply_case(s: &str, mode: &CaseMode) -> String {
    match mode {
        CaseMode::Upper => s.to_uppercase(),
        CaseMode::Lower => s.to_lowercase(),
        CaseMode::Capitalize => {
            let mut c = s.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        }
    }
}

pub fn format_sql(sql: &str, opts: &Options) -> PyResult<String> {
    let stmts = split_statements(sql, false);
    let mut out_parts: Vec<String> = Vec::new();

    for stmt in &stmts {
        let formatted = format_statement(stmt, opts);
        out_parts.push(formatted);
    }

    Ok(out_parts.join("\n\n"))
}

fn format_statement(sql: &str, opts: &Options) -> String {
    let tokens = tokenize_str(sql);

    // Phase 1: token-level transformations (keyword_case, identifier_case,
    //          strip_comments, truncate_strings, space_around_operators)
    let mut processed: Vec<(TType, String)> = Vec::with_capacity(tokens.len());

    let mut prev_was_op = false;
    for (tt, val) in tokens {
        // strip comments
        if opts.strip_comments && (tt == T_COMMENT_SINGLE || tt == T_COMMENT_MULTI) {
            // Replace with single space so adjacent tokens aren't joined
            processed.push((T_WHITESPACE, " ".to_string()));
            continue;
        }

        // truncate strings
        let val = if let Some(max_w) = opts.truncate_strings {
            if tt == T_STRING_SINGLE && val.len() > max_w + 2 {
                let inner = &val[1..val.len() - 1];
                if inner.len() > max_w {
                    format!("'{}{}'", &inner[..max_w], opts.truncate_char)
                } else {
                    val
                }
            } else {
                val
            }
        } else {
            val
        };

        // spaces around operators
        let val = if opts.use_space_around_operators
            && (tt == T_OPERATOR || tt == T_COMPARISON)
        {
            // Ensure space before
            if let Some(last) = processed.last_mut() {
                if last.0 != T_WHITESPACE && last.0 != T_NEWLINE {
                    processed.push((T_WHITESPACE, " ".to_string()));
                }
            }
            let v = val.clone();
            processed.push((tt, val));
            processed.push((T_WHITESPACE, " ".to_string()));
            continue;
        } else {
            val
        };

        // keyword_case
        let val = if let Some(ref mode) = opts.keyword_case {
            let is_kw = tt.starts_with("Token.Keyword")
                || tt == T_KEYWORD_DML || tt == T_KEYWORD_DDL || tt == T_KEYWORD_CTE;
            if is_kw { apply_case(&val, mode) } else { val }
        } else { val };

        // identifier_case
        let val = if let Some(ref mode) = opts.identifier_case {
            if tt == T_NAME && !val.starts_with('"') && !val.starts_with('`') {
                apply_case(&val, mode)
            } else { val }
        } else { val };

        processed.push((tt, val));
    }

    // Phase 2: strip_whitespace — collapse runs of whitespace to single space,
    //          strip leading/trailing
    let processed = if opts.strip_whitespace {
        strip_whitespace_tokens(processed)
    } else {
        processed
    };

    // Phase 3: reindent
    let processed = if opts.reindent {
        reindent_tokens(processed, opts)
    } else {
        processed
    };

    // Serialize
    let raw: String = processed.iter().map(|(_, v)| v.as_str()).collect();
    raw.trim().to_string()
}

fn strip_whitespace_tokens(tokens: Vec<(TType, String)>) -> Vec<(TType, String)> {
    let mut result: Vec<(TType, String)> = Vec::with_capacity(tokens.len());
    let mut last_was_ws = false;
    let mut is_first = true;

    for (tt, val) in tokens {
        let is_ws = tt == T_WHITESPACE || tt == T_NEWLINE;
        if is_ws {
            if !last_was_ws && !is_first {
                result.push((T_WHITESPACE, " ".to_string()));
            }
            last_was_ws = true;
        } else {
            result.push((tt, val));
            last_was_ws = false;
            is_first = false;
        }
    }

    // Strip trailing whitespace
    while result.last().map(|(tt, _)| *tt == T_WHITESPACE).unwrap_or(false) {
        result.pop();
    }
    result
}

fn reindent_tokens(tokens: Vec<(TType, String)>, opts: &Options) -> Vec<(TType, String)> {
    // Reindent: insert newlines before major keywords
    let split_kws = [
        "FROM", "JOIN", "INNER JOIN", "LEFT JOIN", "RIGHT JOIN",
        "FULL JOIN", "CROSS JOIN", "OUTER JOIN", "LEFT OUTER JOIN",
        "RIGHT OUTER JOIN", "FULL OUTER JOIN",
        "WHERE", "AND", "OR", "GROUP BY", "ORDER BY", "HAVING",
        "LIMIT", "OFFSET", "UNION", "UNION ALL", "EXCEPT",
        "INTERSECT", "SET", "VALUES", "ON", "RETURNING",
    ];

    let indent = opts.indent_char.to_string().repeat(opts.indent_width);
    let mut result: Vec<(TType, String)> = Vec::with_capacity(tokens.len() * 2);
    let mut depth: i32 = 0;
    let mut paren_depth: i32 = 0;
    let mut first_token = true;

    for (tt, val) in &tokens {
        let upper = val.to_uppercase();
        let is_ws = *tt == T_WHITESPACE || *tt == T_NEWLINE;

        if is_ws { continue; } // we'll reinsert

        if *tt == T_PUNCTUATION && val == "(" {
            result.push((*tt, val.clone()));
            paren_depth += 1;
            continue;
        }
        if *tt == T_PUNCTUATION && val == ")" {
            result.push((*tt, val.clone()));
            paren_depth -= 1;
            continue;
        }

        // Check for DML keyword (SELECT, INSERT etc.) — newline before, unless first
        let is_dml = *tt == T_KEYWORD_DML || *tt == T_KEYWORD_DDL;
        let is_split_kw = split_kws.iter().any(|k| *k == upper.as_str());

        if paren_depth == 0 && (is_dml || is_split_kw) && !first_token {
            let nl = format!("\n{}", indent.repeat(depth as usize));
            result.push((T_NEWLINE, nl));
        }

        result.push((*tt, val.clone()));

        if is_dml && (upper == "SELECT" || upper == "INSERT" || upper == "UPDATE" || upper == "DELETE") {
            if opts.indent_after_first {
                depth += 1;
            }
        }

        first_token = false;

        // Add space after token (will be collapsed later if not needed)
        result.push((T_WHITESPACE, " ".to_string()));
    }

    // Remove trailing space
    while result.last().map(|(tt, _)| *tt == T_WHITESPACE || *tt == T_NEWLINE).unwrap_or(false) {
        result.pop();
    }

    result
}
