// src/lib.rs — Rust core for sqlparse drop-in replacement
// Exposes the module as `sqlparse._rust`

use once_cell::sync::Lazy;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use regex::Regex;
use std::collections::HashMap;

mod keywords;
mod lexer;
mod splitter;
mod grouping;
mod formatter;
mod sql_types;

use lexer::tokenize_str;
use splitter::split_statements;
use sql_types::*;

// ── public Python functions ────────────────────────────────────────────────

/// Tokenize SQL into (ttype_str, value) pairs.
#[pyfunction]
fn tokenize(py: Python<'_>, sql: &str) -> PyResult<Vec<(PyObject, String)>> {
    let tokens = tokenize_str(sql);
    tokens
        .into_iter()
        .map(|(tt, val)| Ok((ttype_to_py(py, &tt)?, val)))
        .collect()
}

/// Split SQL string into individual Statement objects.
#[pyfunction]
#[pyo3(signature = (sql, encoding=None, strip_semicolon=false))]
fn split(
    py: Python<'_>,
    sql: &str,
    encoding: Option<&str>,
    strip_semicolon: bool,
) -> PyResult<Vec<String>> {
    let _ = encoding; // encoding ignored: Rust strings are always UTF-8
    let stmts = split_statements(sql, strip_semicolon);
    Ok(stmts
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect())
}

/// Parse SQL and return a tuple of Statement objects.
#[pyfunction]
#[pyo3(signature = (sql, encoding=None))]
fn parse(py: Python<'_>, sql: &str, encoding: Option<&str>) -> PyResult<PyObject> {
    let _ = encoding;
    let raw_stmts = split_statements(sql, false);
    let list = PyList::empty(py);
    for raw in &raw_stmts {
        let stmt = build_statement(py, raw)?;
        list.append(stmt)?;
    }
    // return tuple (matches sqlparse behaviour)
    Ok(PyTuple::new(py, list.iter())?.into())
}

/// Format SQL according to options.
#[pyfunction]
#[pyo3(signature = (sql, encoding=None, **options))]
fn format(
    py: Python<'_>,
    sql: &str,
    encoding: Option<&str>,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<String> {
    let _ = encoding;
    let opts = formatter::Options::from_pydict(py, options)?;
    formatter::format_sql(sql, &opts)
}

// ── module registration ────────────────────────────────────────────────────

#[pymodule]
fn sqlparse_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(tokenize, m)?)?;
    m.add_function(wrap_pyfunction!(split, m)?)?;
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    m.add_function(wrap_pyfunction!(format, m)?)?;

    // Register Python-visible classes
    m.add_class::<PyToken>()?;
    m.add_class::<PyTokenList>()?;
    m.add_class::<PyStatement>()?;
    m.add_class::<PyIdentifier>()?;
    m.add_class::<PyIdentifierList>()?;
    m.add_class::<PyParenthesis>()?;
    m.add_class::<PyComparison>()?;
    m.add_class::<PyWhere>()?;
    m.add_class::<PyFunction>()?;
    m.add_class::<PyComment>()?;

    Ok(())
}
