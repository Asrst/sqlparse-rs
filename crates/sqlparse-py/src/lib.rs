//! PyO3 wrapper — exposes the Rust hot paths under the `sqlparse_rs` module.
//!
//!   tokenize(sql) -> list[(ttype_name, value)]
//!   split(sql)    -> list[str]
//!
//! Token type names are the exact `str(ttype)` strings Python sqlparse
//! emits, so results can be compared 1:1 against the reference
//! implementation (see tests/test_wrapper.py).

use pyo3::prelude::*;

use sqlparse_core;

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

#[pymodule]
fn sqlparse_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(tokenize, m)?)?;
    m.add_function(wrap_pyfunction!(split, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
