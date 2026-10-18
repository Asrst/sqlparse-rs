---
name: pyo3-maturin
description: Use when working on PyO3 bindings, maturin builds, Rust↔Python type mapping, or pyclass/pymethods patterns in this project. Covers the pyo3 0.22 API, maturin commands, and sqlparse-rs specific conventions.
---

# PyO3 + Maturin Reference for sqlparse-rs

## Project-specific config

```toml
# Cargo.toml — extension module (cdylib), no std binary
[lib]
crate-type = ["cdylib"]

[dependencies]
pyo3 = { version = "0.22", features = ["extension-module"] }
```

```toml
# pyproject.toml — maturin build backend
[tool.maturin]
features = ["pyo3/extension-module"]
module-name = "sqlparse._rust"   # installed as sqlparse/_rust.so
python-source = "python"          # Python shim lives in python/
```

## Key maturin commands

```bash
source "$HOME/.cargo/env"          # activate Rust toolchain (this devcontainer)
maturin develop                    # build debug .so, install into current venv
maturin develop --release          # build optimised .so
maturin build --release            # produce wheel in target/wheels/
maturin publish                    # upload wheel to PyPI
```

## PyO3 0.22 API — critical patterns

### `Bound<'_, T>` is the new API (replaces `&T` / `PyRef`)
```rust
// NEW (0.22+) — preferred
#[pyfunction]
fn my_fn(py: Python<'_>, val: &Bound<'_, PyAny>) -> PyResult<String> { ... }

// OLD (still compiles but deprecated)
#[pyfunction]
fn my_fn(py: Python<'_>, val: &PyAny) -> PyResult<String> { ... }
```

### `#[pyclass]` basics
```rust
#[pyclass]
struct MyToken {
    #[pyo3(get, set)]  // exposes as Python attribute
    pub value: String,
}

#[pymethods]
impl MyToken {
    #[new]
    fn new(value: String) -> Self { MyToken { value } }

    fn __str__(&self) -> &str { &self.value }
    fn __repr__(&self) -> String { format!("Token({})", self.value) }
}
```

### Subclassing (`extends=`)
```rust
// Base
#[pyclass]
struct PyTokenList { pub tokens: Vec<PyObject> }

// Subclass
#[pyclass(extends = PyTokenList)]
struct PyStatement {}

#[pymethods]
impl PyStatement {
    #[new]
    fn new(tokens: Vec<PyObject>) -> (Self, PyTokenList) {
        (PyStatement {}, PyTokenList { tokens })
    }
    // Access base fields via second arg in methods:
    fn get_type(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<String> {
        let base = slf.as_ref(); // &PyTokenList
        // ...
    }
}
```

### Returning `PyObject` (when type is dynamic)
```rust
// Wrap a concrete Rust type into a Python object
let obj: PyObject = Py::new(py, MyToken::new("SELECT".into()))?.into_py(py);

// Return Python None
Ok(py.None())

// Clone a reference without GIL transfer
let cloned = obj.clone_ref(py);
```

### Calling Python methods on `PyObject` from Rust
```rust
let val: String = tok.getattr(py, "value")?.extract(py)?;
let is_ws: bool = tok.getattr(py, "is_whitespace")?.extract(py).unwrap_or(false);
tok.call_method0(py, "flatten")?.extract::<Vec<PyObject>>(py)?;
```

### `PyList` / `PyTuple`
```rust
use pyo3::types::{PyList, PyTuple};
let list = PyList::empty(py);
list.append(my_obj)?;
let tuple = PyTuple::new(py, list.iter())?;  // returns Bound<'_, PyTuple>
let tuple_obj: PyObject = tuple.into();
```

### Module registration
```rust
#[pymodule]
fn sqlparse_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(my_fn, m)?)?;
    m.add_class::<MyToken>()?;
    Ok(())
}
```

## Type mapping cheatsheet

| Python | Rust (extract) | Rust (return) |
|--------|---------------|---------------|
| `str` | `&str` / `String` | `String` / `&str` |
| `bool` | `bool` | `bool` |
| `int` | `i64` / `usize` | `i64` |
| `list[str]` | `Vec<String>` | `Vec<String>` |
| `tuple` | `(A, B)` | `(A, B)` |
| `dict` | `&Bound<'_, PyDict>` | `PyDict` |
| `None` | `Option<T>` | `py.None()` |
| any object | `PyObject` | `PyObject` |

## Common errors

| Error | Fix |
|-------|-----|
| `E0277: the trait bound PyTokenList: PyClass is not satisfied` | Add `#[pyclass]` to base type |
| `cannot return value referencing local data` | Use `Py<T>` (owned) not `&T` (borrowed) |
| `module 'sqlparse' has no attribute '_rust'` | Run `maturin develop` first; check `module-name` in pyproject.toml |
| `ImportError: dynamic module does not define init function` | `crate-type` must include `"cdylib"` |
| GIL-related deadlock | Never hold `Python<'_>` token while calling back into user Python |

## Gotchas specific to this project

- `module-name = "sqlparse._rust"` means the `.so` is installed **inside** the `sqlparse` package — the Python shim at `python/sqlparse/__init__.py` imports it as `from sqlparse import _rust`.
- `python-source = "python"` tells maturin the Python package root is `./python/`, not `./`.
- `once_cell::sync::Lazy` is used for static compiled-once regex and keyword maps — do not reinitialize them per call.
- The `ttype_str` field is a `&'static str` path like `"Token.Keyword.DML"`. Python-side `.ttype` is a dynamic attribute resolved by traversing `sqlparse.tokens.Token` attribute chain.
