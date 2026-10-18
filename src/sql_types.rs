// src/sql_types.rs — PyO3 classes mirroring sqlparse.sql.*
// Token, TokenList, Statement, Identifier, IdentifierList,
// Parenthesis, Comparison, Where, Function, Comment

use pyo3::prelude::*;
use pyo3::types::{PyList, PyString, PyTuple};

use crate::keywords::*;
use crate::lexer::tokenize_str;
use crate::grouping::group_tokens;

// ── ttype helper: convert &'static str path to Python object ─────────────

/// Convert a TType string like "Token.Keyword.DML" into the Python
/// sqlparse.tokens._TokenType object by traversing attributes on the
/// `sqlparse.tokens.Token` root.
pub fn ttype_to_py(py: Python<'_>, ttype: &str) -> PyResult<PyObject> {
    let tokens_mod = py.import("sqlparse.tokens")?;
    let mut obj: PyObject = tokens_mod.getattr("Token")?.into();
    // The first segment is always "Token" — skip it
    for part in ttype.split('.').skip(1) {
        obj = obj.getattr(py, part)?;
    }
    Ok(obj)
}

// ── Token ────────────────────────────────────────────────────────────────

#[pyclass(name = "Token")]
#[derive(Clone)]
pub struct PyToken {
    #[pyo3(get, set)]
    pub ttype_str: String, // internal, e.g. "Token.Keyword.DML"
    #[pyo3(get, set)]
    pub value: String,
    #[pyo3(get, set)]
    pub is_group: bool,
    #[pyo3(get, set)]
    pub is_whitespace: bool,
    #[pyo3(get, set)]
    pub is_keyword: bool,
    #[pyo3(get, set)]
    pub normalized: String,
    // parent is stored as a weak ref in real sqlparse; we skip it for perf
}

#[pymethods]
impl PyToken {
    #[new]
    pub fn new(ttype_str: String, value: String) -> Self {
        let is_ws = ttype_str.starts_with("Token.Text.Whitespace");
        let is_kw = ttype_str.starts_with("Token.Keyword");
        let normalized = if is_kw { value.to_uppercase() } else { value.clone() };
        PyToken {
            ttype_str,
            value,
            is_group: false,
            is_whitespace: is_ws,
            is_keyword: is_kw,
            normalized,
        }
    }

    #[getter]
    fn ttype(&self, py: Python<'_>) -> PyResult<PyObject> {
        ttype_to_py(py, &self.ttype_str)
    }

    fn __str__(&self) -> &str {
        &self.value
    }

    fn __repr__(&self) -> String {
        let cls = self.ttype_str.split('.').last().unwrap_or("Token");
        let short: String = if self.value.len() > 7 {
            format!("{}...", &self.value[..6])
        } else {
            self.value.clone()
        };
        format!("<{} '{}'>", cls, short)
    }

    fn flatten(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<PyObject> {
        // A leaf token yields itself
        let list = PyList::new(py, [slf.into_py(py)])?;
        Ok(list.iter().collect::<Vec<_>>().into_py(py))
    }

    fn match_(&self, ttype_str: &str, values: Option<Vec<String>>, regex: Option<bool>) -> bool {
        let type_matched = self.ttype_str == ttype_str
            || self.ttype_str.starts_with(&format!("{}.", ttype_str));
        if !type_matched {
            return false;
        }
        if let Some(vals) = values {
            if vals.is_empty() { return type_matched; }
            let regex = regex.unwrap_or(false);
            if regex {
                vals.iter().any(|pat| {
                    Regex::new(pat).ok()
                        .map(|re| re.is_match(&self.normalized))
                        .unwrap_or(false)
                })
            } else if self.is_keyword {
                vals.iter().any(|v| v.to_uppercase() == self.normalized)
            } else {
                vals.iter().any(|v| *v == self.value)
            }
        } else {
            type_matched
        }
    }

    #[getter]
    fn is_newline(&self) -> bool {
        self.ttype_str == "Token.Text.Whitespace.Newline"
    }
}

use regex::Regex;

// ── TokenList ────────────────────────────────────────────────────────────

#[pyclass(name = "TokenList", subclass)]
#[derive(Clone)]
pub struct PyTokenList {
    #[pyo3(get)]
    pub tokens: Vec<PyObject>, // list of PyToken or PyTokenList subclasses
    #[pyo3(get, set)]
    pub is_group: bool,
}

#[pymethods]
impl PyTokenList {
    #[new]
    pub fn new(tokens: Vec<PyObject>) -> Self {
        PyTokenList { tokens, is_group: true }
    }

    #[getter]
    fn value(&self, py: Python<'_>) -> PyResult<String> {
        let mut s = String::new();
        for tok in &self.tokens {
            let v: String = tok.getattr(py, "value")?.extract(py)?;
            s.push_str(&v);
        }
        Ok(s)
    }

    fn __str__(&self, py: Python<'_>) -> PyResult<String> {
        self.value(py)
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let v = self.value(py)?;
        let short = if v.len() > 7 { format!("{}...", &v[..6]) } else { v };
        Ok(format!("<TokenList '{}'>", short))
    }

    fn flatten<'py>(&self, py: Python<'py>) -> PyResult<Vec<PyObject>> {
        let mut result = Vec::new();
        for tok in &self.tokens {
            let is_group: bool = tok.getattr(py, "is_group")?.extract(py).unwrap_or(false);
            if is_group {
                let sub: Vec<PyObject> = tok.call_method0(py, "flatten")?.extract(py)?;
                result.extend(sub);
            } else {
                result.push(tok.clone_ref(py));
            }
        }
        Ok(result)
    }

    fn get_type(&self, py: Python<'_>) -> PyResult<String> {
        // Walk tokens to find first DML/DDL keyword
        for tok in &self.tokens {
            let tts: String = tok.getattr(py, "ttype_str")?.extract(py).unwrap_or_default();
            if tts == T_KEYWORD_DML || tts == T_KEYWORD_DDL {
                let n: String = tok.getattr(py, "normalized")?.extract(py).unwrap_or_default();
                return Ok(n);
            }
        }
        Ok("UNKNOWN".to_string())
    }

    fn token_first(&self, py: Python<'_>) -> PyResult<Option<PyObject>> {
        for tok in &self.tokens {
            let is_ws: bool = tok.getattr(py, "is_whitespace")?.extract(py).unwrap_or(false);
            if !is_ws {
                return Ok(Some(tok.clone_ref(py)));
            }
        }
        Ok(None)
    }

    fn get_sublists(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        let mut result = Vec::new();
        for tok in &self.tokens {
            let is_group: bool = tok.getattr(py, "is_group")?.extract(py).unwrap_or(false);
            if is_group {
                result.push(tok.clone_ref(py));
            }
        }
        Ok(result)
    }

    #[getter]
    fn ttype(&self, py: Python<'_>) -> PyResult<PyObject> {
        Ok(py.None())
    }

    #[getter]
    fn ttype_str(&self) -> &str {
        ""
    }

    #[getter]
    fn is_whitespace(&self) -> bool { false }
    #[getter]
    fn is_keyword(&self) -> bool { false }
    #[getter]
    fn normalized(&self, py: Python<'_>) -> PyResult<String> { self.value(py) }
}

// ── Statement ────────────────────────────────────────────────────────────

#[pyclass(name = "Statement", extends = PyTokenList)]
#[derive(Clone)]
pub struct PyStatement {}

#[pymethods]
impl PyStatement {
    #[new]
    pub fn new(tokens: Vec<PyObject>) -> (Self, PyTokenList) {
        (PyStatement {}, PyTokenList::new(tokens))
    }

    fn get_type(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<String> {
        let base = slf.as_ref();
        base.get_type(py)
    }
}

// ── Identifier ───────────────────────────────────────────────────────────

#[pyclass(name = "Identifier", extends = PyTokenList)]
#[derive(Clone)]
pub struct PyIdentifier {}

#[pymethods]
impl PyIdentifier {
    #[new]
    pub fn new(tokens: Vec<PyObject>) -> (Self, PyTokenList) {
        (PyIdentifier {}, PyTokenList::new(tokens))
    }

    fn get_alias(&self, py: Python<'_>, base: PyRef<'_, PyTokenList>) -> PyResult<Option<String>> {
        // Look for AS keyword followed by name
        let tokens = &base.tokens;
        for (i, tok) in tokens.iter().enumerate() {
            let tts: String = tok.getattr(py, "ttype_str")?.extract(py).unwrap_or_default();
            let norm: String = tok.getattr(py, "normalized")?.extract(py).unwrap_or_default();
            if tts == T_KEYWORD && norm == "AS" {
                if let Some(next) = tokens.get(i + 1).or_else(|| tokens.get(i + 2)) {
                    let is_ws: bool = next.getattr(py, "is_whitespace")?.extract(py).unwrap_or(false);
                    let candidate = if is_ws { tokens.get(i + 2) } else { Some(next) };
                    if let Some(c) = candidate {
                        let v: String = c.getattr(py, "value")?.extract(py).unwrap_or_default();
                        return Ok(Some(v.trim_matches('"').trim_matches('`').to_string()));
                    }
                }
            }
        }
        Ok(None)
    }

    fn get_real_name(&self, py: Python<'_>, base: PyRef<'_, PyTokenList>) -> PyResult<Option<String>> {
        for tok in &base.tokens {
            let tts: String = tok.getattr(py, "ttype_str")?.extract(py).unwrap_or_default();
            if tts == T_NAME || tts == T_STRING_SYMBOL {
                let v: String = tok.getattr(py, "value")?.extract(py).unwrap_or_default();
                return Ok(Some(v.trim_matches('"').trim_matches('`').to_string()));
            }
        }
        Ok(None)
    }
}

// ── IdentifierList ───────────────────────────────────────────────────────

#[pyclass(name = "IdentifierList", extends = PyTokenList)]
#[derive(Clone)]
pub struct PyIdentifierList {}

#[pymethods]
impl PyIdentifierList {
    #[new]
    pub fn new(tokens: Vec<PyObject>) -> (Self, PyTokenList) {
        (PyIdentifierList {}, PyTokenList::new(tokens))
    }

    fn get_identifiers(&self, py: Python<'_>, base: PyRef<'_, PyTokenList>) -> PyResult<Vec<PyObject>> {
        let mut result = Vec::new();
        for tok in &base.tokens {
            let is_ws: bool = tok.getattr(py, "is_whitespace")?.extract(py).unwrap_or(false);
            let tts: String = tok.getattr(py, "ttype_str")?.extract(py).unwrap_or_default();
            let val: String = tok.getattr(py, "value")?.extract(py).unwrap_or_default();
            if !is_ws && !(tts == T_PUNCTUATION && val == ",") {
                result.push(tok.clone_ref(py));
            }
        }
        Ok(result)
    }
}

// ── Remaining group types (lightweight) ─────────────────────────────────

macro_rules! simple_group {
    ($name:ident, $pyname:literal) => {
        #[pyclass(name = $pyname, extends = PyTokenList)]
        #[derive(Clone)]
        pub struct $name {}

        #[pymethods]
        impl $name {
            #[new]
            pub fn new(tokens: Vec<PyObject>) -> (Self, PyTokenList) {
                ($name {}, PyTokenList::new(tokens))
            }
        }
    };
}

simple_group!(PyParenthesis, "Parenthesis");
simple_group!(PyComparison, "Comparison");
simple_group!(PyWhere, "Where");
simple_group!(PyFunction, "Function");
simple_group!(PyComment, "Comment");
simple_group!(PyCase, "Case");
simple_group!(PyOperation, "Operation");
simple_group!(PyValues, "Values");

// ── build_statement: tokenize + group into a PyStatement ─────────────────

pub fn build_statement(py: Python<'_>, sql: &str) -> PyResult<PyObject> {
    let raw_tokens = tokenize_str(sql);

    // Convert flat tokens to PyToken objects
    let mut py_tokens: Vec<PyObject> = raw_tokens
        .iter()
        .map(|(tt, val)| {
            let tok = PyToken::new(tt.to_string(), val.clone());
            Py::new(py, tok).map(|p| p.into_py(py))
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Run grouping passes
    py_tokens = group_tokens(py, py_tokens)?;

    // Wrap in Statement
    let stmt = PyStatement::new(py_tokens);
    Ok(Py::new(py, stmt)?.into_py(py))
}
