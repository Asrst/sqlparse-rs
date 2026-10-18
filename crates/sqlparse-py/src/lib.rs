//! PyO3 wrapper — exposes Rust hot paths under the `sqlparse_rs` module.
//!
//!   tokenize(sql) -> list[(ttype_name, value)]
//!   split(sql)    -> list[str]
//!   parse(sql)    -> tuple[Statement, ...]    (Python sqlparse.sql objects)
//!   format(sql, **opts) -> str                (fully in Rust)
//!
//! `parse()` builds the AST in Rust (tokenize → group) and returns native
//! Python `sqlparse.sql.*` objects so callers can use the full sqlparse API
//! (get_type, flatten, get_name, etc.) without any Python tokenization.
//!
//! `format()` is entirely in Rust — tokenize → optionally group → apply
//! filters → serialize.  No Python code is called.

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use pyo3::IntoPyObjectExt;

use sqlparse_core::{self, grouping, formatter};

// ──────────────────────────────────────────────────────────────────────────
// tokenize / split  (Tier 1 + 2 — unchanged)
// ──────────────────────────────────────────────────────────────────────────

#[pyfunction]
fn tokenize(sql: &str) -> Vec<(String, String)> {
    sqlparse_core::tokenize(sql)
        .into_iter()
        .map(|(t, v)| (t.to_string(), v.to_string()))
        .collect()
}

#[pyfunction]
fn split(sql: &str) -> Vec<String> {
    sqlparse_core::split(sql)
}

// ──────────────────────────────────────────────────────────────────────────
// parse  (Tier 3 — Rust grouping → Python sqlparse.sql objects)
// ──────────────────────────────────────────────────────────────────────────

/// Build a Python `sqlparse.sql.Statement` tree from a pre-grouped Rust node.
fn node_to_py(py: Python<'_>, node: &grouping::Node) -> PyResult<PyObject> {
    use grouping::NodeKind;

    let sql_mod  = py.import("sqlparse_rs.sql")?;
    let tok_mod  = py.import("sqlparse_rs.tokens")?;

    if node.is_leaf() {
        // Leaf → sqlparse.sql.Token(ttype, value)
        let ttype_str = node.ttype.unwrap_or("Token.Error");
        let ttype_obj = resolve_ttype(py, &tok_mod, ttype_str)?;
        let tok_cls = sql_mod.getattr("Token")?;
        return tok_cls.call1((ttype_obj, node.value.as_str()))?.into_py_any(py);
    }

    // Group → appropriate sqlparse.sql.* class.
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
/// `sqlparse.tokens._TokenType` object.
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

    // Split into individual statements first, then build AST for each.
    let stmts_raw = sqlparse_core::split(sql);
    // If split returns nothing, there may be one empty statement.
    let stmts_sql: Vec<String> = if stmts_raw.is_empty() && !sql.trim().is_empty() {
        vec![sql.to_owned()]
    } else {
        stmts_raw
    };

    // Handle edge case: empty input → single empty Statement.
    if sql.trim().is_empty() {
        let sql_mod = py.import("sqlparse_rs.sql")?;
        let empty_list = PyList::empty(py);
        let stmt = sql_mod.getattr("Statement")?.call1((empty_list,))?;
        let tup = PyTuple::new(py, &[stmt])?;
        return tup.into_py_any(py);
    }

    let py_stmts: Vec<PyObject> = stmts_sql.iter().map(|s| -> PyResult<PyObject> {
        let raw = sqlparse_core::tokenize(s);
        let owned: Vec<(String, String)> = raw.into_iter()
            .map(|(t, v)| (t.to_string(), v.to_string()))
            .collect();
        let mut stmt_node = grouping::tokens_to_statement(owned);
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
    py: Python<'_>,
    sql: &str,
    encoding: Option<&str>,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<String> {
    let _ = (py, encoding);
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
    m.add_function(wrap_pyfunction!(tokenize, m)?)?;
    m.add_function(wrap_pyfunction!(split, m)?)?;
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    m.add_function(wrap_pyfunction!(format, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
