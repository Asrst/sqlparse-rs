// src/grouping.rs — Token grouping passes
// Ported from sqlparse/engine/grouping.py
// Groups flat token streams into nested structures.

use pyo3::prelude::*;
use pyo3::types::PyList;

use crate::keywords::*;

// ── helpers ──────────────────────────────────────────────────────────────

fn get_ttype(py: Python<'_>, tok: &PyObject) -> String {
    tok.getattr(py, "ttype_str").ok()
        .and_then(|v| v.extract::<String>(py).ok())
        .unwrap_or_default()
}

fn get_value(py: Python<'_>, tok: &PyObject) -> String {
    tok.getattr(py, "value").ok()
        .and_then(|v| v.extract::<String>(py).ok())
        .unwrap_or_default()
}

fn is_whitespace(py: Python<'_>, tok: &PyObject) -> bool {
    tok.getattr(py, "is_whitespace").ok()
        .and_then(|v| v.extract::<bool>(py).ok())
        .unwrap_or(false)
}

fn is_group(py: Python<'_>, tok: &PyObject) -> bool {
    tok.getattr(py, "is_group").ok()
        .and_then(|v| v.extract::<bool>(py).ok())
        .unwrap_or(false)
}

fn make_group(py: Python<'_>, class: &str, tokens: Vec<PyObject>) -> PyResult<PyObject> {
    use crate::sql_types::*;
    match class {
        "Parenthesis" => Ok(Py::new(py, PyParenthesis::new(tokens))?.into_py(py)),
        "Identifier"  => Ok(Py::new(py, PyIdentifier::new(tokens))?.into_py(py)),
        "IdentifierList" => Ok(Py::new(py, PyIdentifierList::new(tokens))?.into_py(py)),
        "Where"       => Ok(Py::new(py, PyWhere::new(tokens))?.into_py(py)),
        "Comparison"  => Ok(Py::new(py, PyComparison::new(tokens))?.into_py(py)),
        "Function"    => Ok(Py::new(py, PyFunction::new(tokens))?.into_py(py)),
        "Comment"     => Ok(Py::new(py, PyComment::new(tokens))?.into_py(py)),
        "Case"        => Ok(Py::new(py, PyCase::new(tokens))?.into_py(py)),
        "Operation"   => Ok(Py::new(py, PyOperation::new(tokens))?.into_py(py)),
        "Values"      => Ok(Py::new(py, PyValues::new(tokens))?.into_py(py)),
        _ => Ok(Py::new(py, crate::sql_types::PyTokenList::new(tokens))?.into_py(py)),
    }
}

// ── matching-based grouping (parentheses, CASE, WHERE, etc.) ────────────

struct MatchSpec {
    open_ttype: &'static str,
    open_value: &'static str,
    close_ttype: &'static str,
    close_value: &'static str,
    group_class: &'static str,
}

fn group_matching(
    py: Python<'_>,
    tokens: Vec<PyObject>,
    spec: &MatchSpec,
) -> PyResult<Vec<PyObject>> {
    let mut result: Vec<PyObject> = Vec::with_capacity(tokens.len());
    let mut opens: Vec<usize> = Vec::new(); // positions in result

    for tok in tokens {
        let tt = get_ttype(py, &tok);
        let val = get_value(py, &tok);
        let upper = val.to_uppercase();

        let is_open = tt == spec.open_ttype && upper == spec.open_value.to_uppercase();
        let is_close = tt == spec.close_ttype && upper == spec.close_value.to_uppercase();

        result.push(tok);

        if is_open {
            opens.push(result.len() - 1);
        } else if is_close {
            if let Some(open_idx) = opens.pop() {
                // Collect tokens from open_idx..=current into group
                let close_idx = result.len() - 1;
                let group_tokens: Vec<PyObject> = result
                    .drain(open_idx..)
                    .collect();
                let grp = make_group(py, spec.group_class, group_tokens)?;
                result.push(grp);
            }
        }
    }
    Ok(result)
}

// ── parenthesis grouping ─────────────────────────────────────────────────

fn group_parenthesis(py: Python<'_>, tokens: Vec<PyObject>) -> PyResult<Vec<PyObject>> {
    group_matching(py, tokens, &MatchSpec {
        open_ttype:   T_PUNCTUATION,  open_value:  "(",
        close_ttype:  T_PUNCTUATION,  close_value: ")",
        group_class: "Parenthesis",
    })
}

// ── WHERE grouping ───────────────────────────────────────────────────────

fn group_where(py: Python<'_>, tokens: Vec<PyObject>) -> PyResult<Vec<PyObject>> {
    let close_kws = [
        "ORDER BY", "GROUP BY", "LIMIT", "UNION", "UNION ALL",
        "EXCEPT", "INTERSECT", "HAVING", "RETURNING", "INTO",
    ];
    let mut result: Vec<PyObject> = Vec::with_capacity(tokens.len());
    let mut in_where = false;
    let mut where_start: usize = 0;

    let n = tokens.len();
    let mut i = 0;
    while i < n {
        let tok = &tokens[i];
        let tt = get_ttype(py, tok);
        let val = get_value(py, tok);
        let upper = val.to_uppercase();

        if !in_where && tt == T_KEYWORD && upper == "WHERE" {
            in_where = true;
            where_start = result.len();
            result.push(tokens[i].clone_ref(py));
            i += 1;
            continue;
        }

        if in_where && tt == T_KEYWORD && close_kws.iter().any(|k| *k == upper) {
            // Close WHERE group
            let group_toks: Vec<PyObject> = result.drain(where_start..).collect();
            let grp = make_group(py, "Where", group_toks)?;
            result.push(grp);
            in_where = false;
        }

        result.push(tokens[i].clone_ref(py));
        i += 1;
    }

    if in_where {
        let group_toks: Vec<PyObject> = result.drain(where_start..).collect();
        let grp = make_group(py, "Where", group_toks)?;
        result.push(grp);
    }

    Ok(result)
}

// ── comparison grouping ──────────────────────────────────────────────────

fn group_comparisons(py: Python<'_>, tokens: Vec<PyObject>) -> PyResult<Vec<PyObject>> {
    let mut result: Vec<PyObject> = Vec::with_capacity(tokens.len());
    let mut i = 0;
    let n = tokens.len();

    while i < n {
        let tt = get_ttype(py, &tokens[i]);
        if tt == T_COMPARISON {
            // Find prev non-ws
            let mut prev_idx = None;
            for j in (0..result.len()).rev() {
                if !is_whitespace(py, &result[j]) {
                    prev_idx = Some(j);
                    break;
                }
            }
            // Find next non-ws
            let mut next_idx = None;
            for j in (i + 1)..n {
                if !is_whitespace(py, &tokens[j]) {
                    next_idx = Some(j);
                    break;
                }
            }

            if let (Some(pi), Some(ni)) = (prev_idx, next_idx) {
                // Drain from prev_idx..=result.len() (prev + any trailing ws + op)
                // plus tokens[i+1..=ni]
                let start = pi;
                let mut group_toks: Vec<PyObject> = result.drain(start..).collect();
                // add operator
                group_toks.push(tokens[i].clone_ref(py));
                // add tokens between operator and next non-ws (whitespace)
                for j in (i + 1)..=ni {
                    group_toks.push(tokens[j].clone_ref(py));
                }
                let grp = make_group(py, "Comparison", group_toks)?;
                result.push(grp);
                i = ni + 1;
                continue;
            }
        }
        result.push(tokens[i].clone_ref(py));
        i += 1;
    }
    Ok(result)
}

// ── identifier grouping ──────────────────────────────────────────────────

fn group_identifiers(py: Python<'_>, tokens: Vec<PyObject>) -> PyResult<Vec<PyObject>> {
    let mut result: Vec<PyObject> = Vec::with_capacity(tokens.len());
    for tok in tokens {
        let tt = get_ttype(py, &tok);
        if tt == T_NAME || tt == T_STRING_SYMBOL {
            // Wrap bare name in Identifier
            let grp = make_group(py, "Identifier", vec![tok])?;
            result.push(grp);
        } else {
            result.push(tok);
        }
    }
    Ok(result)
}

// ── function grouping (Name followed by Parenthesis) ────────────────────

fn group_functions(py: Python<'_>, tokens: Vec<PyObject>) -> PyResult<Vec<PyObject>> {
    let mut result: Vec<PyObject> = Vec::with_capacity(tokens.len());
    let mut i = 0;
    let n = tokens.len();

    while i < n {
        let tt = get_ttype(py, &tokens[i]);
        let is_ident = {
            let cls = tokens[i].getattr(py, "__class__")
                .and_then(|c| c.getattr(py, "__name__"))
                .and_then(|n| n.extract::<String>(py))
                .unwrap_or_default();
            cls == "Identifier"
        };

        // Check if this is a bare Name followed by Parenthesis
        if tt == T_NAME {
            if let Some(next) = tokens.get(i + 1) {
                let next_cls = next.getattr(py, "__class__")
                    .and_then(|c| c.getattr(py, "__name__"))
                    .and_then(|n| n.extract::<String>(py))
                    .unwrap_or_default();
                if next_cls == "Parenthesis" {
                    let grp = make_group(py, "Function", vec![
                        tokens[i].clone_ref(py),
                        tokens[i + 1].clone_ref(py),
                    ])?;
                    result.push(grp);
                    i += 2;
                    continue;
                }
            }
        }
        result.push(tokens[i].clone_ref(py));
        i += 1;
    }
    Ok(result)
}

// ── identifier list (comma-separated) ───────────────────────────────────

fn group_identifier_list(py: Python<'_>, tokens: Vec<PyObject>) -> PyResult<Vec<PyObject>> {
    // Find runs of identifiers/values separated by commas
    let mut result: Vec<PyObject> = Vec::with_capacity(tokens.len());
    let mut i = 0;
    let n = tokens.len();

    while i < n {
        let tt = get_ttype(py, &tokens[i]);
        let val = get_value(py, &tokens[i]);
        let is_comma = tt == T_PUNCTUATION && val == ",";

        if is_comma && !result.is_empty() {
            // Look back for the identifier group start
            let mut start = result.len();
            // Walk back past the last identifier/value/whitespace cluster
            while start > 0 {
                let prev_cls = result[start - 1].getattr(py, "__class__")
                    .and_then(|c| c.getattr(py, "__name__"))
                    .and_then(|nm| nm.extract::<String>(py))
                    .unwrap_or_default();
                let prev_tt = get_ttype(py, &result[start - 1]);
                if prev_cls == "Identifier" || prev_cls == "Function"
                    || prev_cls == "Parenthesis" || prev_cls == "Operation"
                    || prev_cls == "Comparison"
                    || prev_tt == T_WILDCARD || prev_tt == T_NUMBER_INT
                    || prev_tt == T_NUMBER_FLOAT || prev_tt == T_STRING_SINGLE
                    || prev_tt == T_KEYWORD || is_whitespace(py, &result[start - 1])
                {
                    start -= 1;
                } else {
                    break;
                }
            }
            // Check if we already have an IdentifierList at that position
            if start < result.len() {
                let first_cls = result[start].getattr(py, "__class__")
                    .and_then(|c| c.getattr(py, "__name__"))
                    .and_then(|nm| nm.extract::<String>(py))
                    .unwrap_or_default();
                if first_cls == "IdentifierList" {
                    // Extend existing list
                    let existing_list_toks: Vec<PyObject> = result
                        .last()
                        .unwrap()
                        .getattr(py, "tokens")
                        .and_then(|t| t.extract::<Vec<PyObject>>(py))
                        .unwrap_or_default();
                    // Rebuild: existing + comma + rest up to next non-list item
                    let mut new_toks = existing_list_toks;
                    new_toks.push(tokens[i].clone_ref(py)); // comma
                    result.push(tokens[i].clone_ref(py));
                    i += 1;
                    continue;
                }

                let mut group_toks: Vec<PyObject> = result.drain(start..).collect();
                group_toks.push(tokens[i].clone_ref(py)); // comma

                // Collect following identifiers
                let mut j = i + 1;
                while j < n {
                    let jtt = get_ttype(py, &tokens[j]);
                    let jcls = tokens[j].getattr(py, "__class__")
                        .and_then(|c| c.getattr(py, "__name__"))
                        .and_then(|nm| nm.extract::<String>(py))
                        .unwrap_or_default();
                    let jval = get_value(py, &tokens[j]);
                    let stop = jtt == T_PUNCTUATION && jval != ",";
                    if stop { break; }
                    group_toks.push(tokens[j].clone_ref(py));
                    let is_next_comma = jtt == T_PUNCTUATION && jval == ",";
                    if is_next_comma {
                        j += 1;
                        continue;
                    }
                    j += 1;
                    // After a non-comma non-ws token, peek ahead for comma
                    let peeked_comma = tokens.get(j)
                        .map(|t| get_ttype(py, t) == T_PUNCTUATION && get_value(py, t) == ",")
                        .unwrap_or(false);
                    if !peeked_comma {
                        break;
                    }
                }
                let grp = make_group(py, "IdentifierList", group_toks)?;
                result.push(grp);
                i = j;
                continue;
            }
        }

        result.push(tokens[i].clone_ref(py));
        i += 1;
    }
    Ok(result)
}

// ── top-level entry point ────────────────────────────────────────────────

/// Run all grouping passes on a flat token list, return grouped tokens.
pub fn group_tokens(py: Python<'_>, tokens: Vec<PyObject>) -> PyResult<Vec<PyObject>> {
    let tokens = group_parenthesis(py, tokens)?;
    let tokens = group_functions(py, tokens)?;
    let tokens = group_where(py, tokens)?;
    let tokens = group_identifiers(py, tokens)?;
    let tokens = group_comparisons(py, tokens)?;
    let tokens = group_identifier_list(py, tokens)?;
    Ok(tokens)
}
