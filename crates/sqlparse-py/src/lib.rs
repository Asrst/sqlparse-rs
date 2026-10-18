//! PyO3 wrapper — exposes Rust hot paths under the `sqlparse_rs` module.
//!
//!   tokenize(sql) -> list[(ttype_name, value)]
//!   split(sql)    -> list[str]
//!   parse(sql)    -> tuple[Statement, ...]    (Python sqlparse.sql objects)
//!   format(sql, **opts) -> str                (fully in Rust)
//!
//! `parse()` builds the AST in Rust (tokenize → group) and returns native
//! `sqlparse_rs.sql.*` objects so callers can use the full sqlparse API
//! (get_type, flatten, get_name, etc.) without any Python tokenization.
//!
//! `format()` is entirely in Rust — tokenize → optionally group → apply
//! filters → serialize.  No Python code is called.
//!
//! Perf note (tokenize): ttype strings come from a fixed set of ~30 static
//! names, so the Python string for each is built once per call and reused
//! for every token; only the value strings are freshly allocated. This cuts
//! tokenize() end-to-end time substantially on large inputs (2.79M-token
//! dump: ~2s of str/tuple construction down to value-only allocation).

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyString, PyTuple};
use pyo3::IntoPyObjectExt;

use sqlparse_core::{self, formatter, grouping};

// ──────────────────────────────────────────────────────────────────────────
// tokenize / split  (Tier 1 + 2 — fast wrapper, cached ttype PyStrings)
// ──────────────────────────────────────────────────────────────────────────

fn tokenize<'py>(py: Python<'py>, sql: &'py str) -> PyResult<Vec<Bound<'py, PyTuple>>> {
    let mut cache: Vec<(&'static str, Bound<'py, PyString>)> = Vec::with_capacity(48);
    let mut out = Vec::new();
    for (t, v) in sqlparse_core::tokenize(sql) {
        // linear scan over ~30 entries beats hashing per token
        let tt = match cache.iter().find(|(name, _)| *name == t) {
            Some((_, s)) => s.clone(),
            None => {
                let s = PyString::new(py, t);
                cache.push((t, s.clone()));
                s
            }
        };
        out.push(PyTuple::new(py, [tt, PyString::new(py, v)])?);
    }
    Ok(out)
}

#[pyfunction(name = "tokenize")]
fn py_tokenize<'a>(py: Python<'a>, sql: &'a str) -> PyResult<Vec<Bound<'a, PyTuple>>> {
    tokenize(py, sql)
}

#[pyfunction]
#[pyo3(signature = (sql, strip_semicolon=false))]
fn split(sql: &str, strip_semicolon: bool) -> Vec<String> {
    if !strip_semicolon {
        return sqlparse_core::split(sql)
            .into_iter()
            .map(|s| s.to_string())
            .collect();
    }
    // python: FilterStack(strip_semicolon=True) removes ONE trailing ';'
    // Punctuation token per statement, then str(stmt).strip(). Token-level
    // strip keeps a ';'-terminated string literal intact.
    sqlparse_core::splitter::split_tokens(sql)
        .into_iter()
        .map(|mut toks| {
            // Trailing ws/comment tokens ride inside the statement; python's
            // filter stack still strips the ';' after them. Pop trailing
            // ws-like tokens first, then ONE ';' Punctuation.
            while let Some(last) = toks.last() {
                if last.0 == sqlparse_core::ttype::PUNCTUATION && last.1 == ";" {
                    toks.pop();
                    break;
                }
                if last.0 == sqlparse_core::ttype::WHITESPACE
                    || last.0 == sqlparse_core::ttype::NEWLINE
                    || last.0 == sqlparse_core::ttype::COMMENT_SINGLE
                    || last.0 == sqlparse_core::ttype::COMMENT_MULTILINE
                {
                    toks.pop();
                    continue;
                }
                break;
            }
            let s: String = toks.into_iter().map(|(_, v)| v).collect();
            s.trim().to_string()
        })
        .collect()
}

// ──────────────────────────────────────────────────────────────────────────
// parse  (Tier 3 — Rust grouping → Python sqlparse_rs.sql objects)
// ──────────────────────────────────────────────────────────────────────────

/// Build a Python `sqlparse_rs.sql.Statement` tree from a pre-grouped Rust node.
fn node_to_py(py: Python<'_>, node: &grouping::Node) -> PyResult<PyObject> {
    use grouping::NodeKind;

    let sql_mod  = py.import("sqlparse_rs.sql")?;
    let tok_mod  = py.import("sqlparse_rs.tokens")?;

    if node.is_leaf() {
        // Leaf → sql.Token(ttype, value)
        let ttype_str = node.ttype.unwrap_or("Token.Error");
        let ttype_obj = resolve_ttype(py, &tok_mod, ttype_str)?;
        let tok_cls = sql_mod.getattr("Token")?;
        return tok_cls.call1((ttype_obj, node.value.as_str()))?.into_py_any(py);
    }

    // Group → appropriate sql.* class.
    let cls_name = match node.kind {
        NodeKind::Statement      => "Statement",
        NodeKind::Identifier     => "Identifier",
        NodeKind::IdentifierList => "IdentifierList",
        NodeKind::Parenthesis    => "Parenthesis",
        NodeKind::SquareBrackets => "SquareBrackets",
        NodeKind::Assignment     => "Assignment",
        NodeKind::If             => "If",
        NodeKind::For            => "For",
        NodeKind::Comparison     => "Comparison",
        NodeKind::Comment        => "Comment",
        NodeKind::Where          => "Where",
        NodeKind::Over           => "Over",
        NodeKind::Case           => "Case",
        NodeKind::Function       => "Function",
        NodeKind::Begin          => "Begin",
        NodeKind::Operation      => "Operation",
        NodeKind::Values         => "Values",
        NodeKind::TypedLiteral   => "TypedLiteral",
        NodeKind::TokenList      => "TokenList",
        NodeKind::Leaf           => unreachable!(),
    };

    let children_py = PyList::empty(py);
    for child in &node.children {
        children_py.append(node_to_py(py, child)?)?;
    }
    let cls = sql_mod.getattr(cls_name)?;
    cls.call1((children_py,))?.into_py_any(py)
}

/// Resolve a ttype string like `"Token.Keyword.DML"` to the Python
/// `sqlparse_rs.tokens._TokenType` object.
fn resolve_ttype<'py>(
    _py: Python<'py>,
    tok_mod: &Bound<'py, PyModule>,
    ttype_str: &str,
) -> PyResult<PyObject> {
    // Traverse the attribute chain "Token.Keyword.DML" → tok_mod.Token.Keyword.DML
    let mut cur: Bound<'py, PyAny> = tok_mod.getattr("Token")?.into_any();
    for part in ttype_str.split('.').skip(1) {  // skip leading "Token"
        cur = cur.getattr(part)?;
    }
    cur.into_py_any(tok_mod.py())
}

/// `parse(sql, encoding=None) -> tuple[Statement, ...]`
#[pyfunction]
#[pyo3(signature = (sql, encoding=None))]
fn parse(py: Python<'_>, sql: &str, encoding: Option<&str>) -> PyResult<PyObject> {
    let _ = encoding; // encoding handled at tokenize level; UTF-8 already decoded by PyO3

    // Split into statement TOKEN LISTS (unstripped — leading/trailing
    // whitespace stays inside its statement exactly like python's
    // StatementSplitter), so `str(stmt)` round-trips the original SQL and
    // token_first(skip_ws=False) sees the leading whitespace.
    let stmt_toks = sqlparse_core::splitter::split_tokens(sql);
    let stmts_toks: Vec<Vec<(String, String)>> = if stmt_toks.is_empty() && !sql.trim().is_empty() {
        // Degenerate fallback: treat the whole input as one statement.
        sqlparse_core::tokenize(sql)
            .into_iter()
            .map(|(t, v)| vec![(t.to_string(), v.to_string())])
            .collect()
    } else {
        stmt_toks
            .into_iter()
            .map(|toks| {
                toks.into_iter()
                    .map(|(t, v)| (t.to_string(), v))
                    .collect()
            })
            .collect()
    };

    // Handle edge case: empty input → single empty Statement.
    if sql.trim().is_empty() {
        let sql_mod = py.import("sqlparse_rs.sql")?;
        let empty_list = PyList::empty(py);
        let stmt = sql_mod.getattr("Statement")?.call1((empty_list,))?;
        let tup = PyTuple::new(py, &[stmt])?;
        return tup.into_py_any(py);
    }

    let py_stmts: Vec<PyObject> = stmts_toks.iter().map(|toks| -> PyResult<PyObject> {
        let mut stmt_node = grouping::tokens_to_statement(toks.clone());
        grouping::group(&mut stmt_node);
        node_to_py(py, &stmt_node)
    }).collect::<PyResult<Vec<_>>>()?;

    PyTuple::new(py, py_stmts)?.into_py_any(py)
}

// ──────────────────────────────────────────────────────────────────────────
// format  (Tier 3 — fully in Rust)
// ──────────────────────────────────────────────────────────────────────────

/// `format(sql, encoding=None, **options) -> str`
#[pyfunction]
#[pyo3(signature = (sql, encoding=None, **options))]
fn format(
    _py: Python<'_>,
    sql: &str,
    encoding: Option<&str>,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<String> {
    let _ = encoding;
    let mut opts = formatter::FormatOptions::new();

    if let Some(opts_dict) = options {
        parse_format_options(opts_dict, &mut opts)?;
    }

    // Validate option combinations (mirrors formatter.validate_options).
    if opts.indent_columns { opts.reindent = true; }
    if opts.reindent || opts.reindent_aligned { opts.strip_whitespace = true; }

    Ok(formatter::format_sql(sql, &opts))
}

fn parse_format_options(
    d: &Bound<'_, PyDict>,
    opts: &mut formatter::FormatOptions,
) -> PyResult<()> {
    use pyo3::exceptions::PyValueError;

    if let Some(v) = d.get_item("keyword_case")? {
        let s: &str = v.extract()?;
        opts.keyword_case = Some(match s {
            "upper"      => formatter::CaseMode::Upper,
            "lower"      => formatter::CaseMode::Lower,
            "capitalize" => formatter::CaseMode::Capitalize,
            other => return Err(PyValueError::new_err(
                format!("Invalid value for keyword_case: {other:?}"))),
        });
    }
    if let Some(v) = d.get_item("identifier_case")? {
        let s: &str = v.extract()?;
        opts.identifier_case = Some(match s {
            "upper"      => formatter::CaseMode::Upper,
            "lower"      => formatter::CaseMode::Lower,
            "capitalize" => formatter::CaseMode::Capitalize,
            other => return Err(PyValueError::new_err(
                format!("Invalid value for identifier_case: {other:?}"))),
        });
    }
    if let Some(v) = d.get_item("strip_comments")? {
        opts.strip_comments = v.extract()?;
    }
    if let Some(v) = d.get_item("strip_whitespace")? {
        opts.strip_whitespace = v.extract()?;
    }
    if let Some(v) = d.get_item("reindent")? {
        opts.reindent = v.extract()?;
    }
    if let Some(v) = d.get_item("reindent_aligned")? {
        opts.reindent_aligned = v.extract()?;
    }
    if let Some(v) = d.get_item("indent_width")? {
        opts.indent_width = v.extract::<usize>()?;
    }
    if let Some(v) = d.get_item("indent_tabs")? {
        opts.indent_tabs = v.extract()?;
    }
    if let Some(v) = d.get_item("indent_after_first")? {
        opts.indent_after_first = v.extract()?;
    }
    if let Some(v) = d.get_item("indent_columns")? {
        opts.indent_columns = v.extract()?;
    }
    if let Some(v) = d.get_item("wrap_after")? {
        opts.wrap_after = v.extract::<usize>()?;
    }
    if let Some(v) = d.get_item("comma_first")? {
        opts.comma_first = v.extract()?;
    }
    if let Some(v) = d.get_item("compact")? {
        opts.compact = v.extract()?;
    }
    if let Some(v) = d.get_item("use_space_around_operators")? {
        opts.use_space_around_operators = v.extract()?;
    }
    if let Some(v) = d.get_item("truncate_strings")? {
        opts.truncate_strings = Some(v.extract::<usize>()?);
    }
    if let Some(v) = d.get_item("truncate_char")? {
        opts.truncate_char = v.extract()?;
    }
    if let Some(v) = d.get_item("output_format")? {
        let s: &str = v.extract()?;
        opts.output_format = Some(match s {
            "sql"    => formatter::OutputFormat::Sql,
            "python" => formatter::OutputFormat::Python,
            "php"    => formatter::OutputFormat::Php,
            other => return Err(PyValueError::new_err(
                format!("Unknown output format: {other:?}"))),
        });
    }
    if let Some(v) = d.get_item("right_margin")? {
        opts.right_margin = Some(v.extract::<usize>()?);
    }
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────
// Module
// ──────────────────────────────────────────────────────────────────────────

#[pymodule]
fn sqlparse_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(py_tokenize, m)?)?;
    m.add_function(wrap_pyfunction!(split, m)?)?;
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    m.add_function(wrap_pyfunction!(format, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
