//! PyO3 adapter exposing this crate to Python as `semantic_version`,
//! matching the import surface used by the original project's own
//! pytest suite (tests/original/), so it can run unmodified against
//! this Rust implementation.
//!
//! Scope cut (see DECISIONS.md): `partial=True` versions and full
//! Clause-tree structural simplification are not implemented. Affected
//! tests are xfail'd in tests/conftest.py, not modified here.

use pyo3::exceptions::{PyNotImplementedError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyType;
use std::cmp::Ordering;

use crate::spec::{Clause as RClause, SimpleSpec as RSimpleSpec};
use crate::npm_spec::NpmSpec as RNpmSpec;
use crate::version::Version as RVersion;

#[pyclass(name = "Version")]
#[derive(Clone)]
struct PyVersion {
    inner: RVersion,
}

#[pymethods]
impl PyVersion {
    #[new]
    #[pyo3(signature = (version_string, partial=false))]
    fn new(version_string: &str, partial: bool) -> PyResult<Self> {
        if partial {
            return Err(PyNotImplementedError::new_err(
                "partial=True is not implemented in this port (see DECISIONS.md)",
            ));
        }
        RVersion::parse(version_string)
            .map(|inner| PyVersion { inner })
            .map_err(|e| PyValueError::new_err(format!("{:?}", e)))
    }

    #[classmethod]
    fn coerce(_cls: &Bound<'_, PyType>, version_string: &str) -> PyResult<Self> {
        RVersion::coerce(version_string)
            .map(|inner| PyVersion { inner })
            .map_err(|e| PyValueError::new_err(format!("{:?}", e)))
    }

    fn next_major(&self) -> PyVersion {
        PyVersion { inner: self.inner.next_major() }
    }
    fn next_minor(&self) -> PyVersion {
        PyVersion { inner: self.inner.next_minor() }
    }
    fn next_patch(&self) -> PyVersion {
        PyVersion { inner: self.inner.next_patch() }
    }
    fn truncate(&self) -> PyVersion {
        PyVersion { inner: self.inner.truncate() }
    }

    fn __str__(&self) -> String {
        self.inner.to_string()
    }
    fn __repr__(&self) -> String {
        format!("Version('{}')", self.inner)
    }
    fn __richcmp__(&self, other: &PyVersion, op: pyo3::basic::CompareOp) -> bool {
        use pyo3::basic::CompareOp::*;
        let ord = self.inner.sort_key().partial_cmp(&other.inner.sort_key());
        match (op, ord) {
            (Eq, _) => self.inner == other.inner,
            (Ne, _) => self.inner != other.inner,
            (Lt, Some(Ordering::Less)) => true,
            (Le, Some(o)) => o != Ordering::Greater,
            (Gt, Some(Ordering::Greater)) => true,
            (Ge, Some(o)) => o != Ordering::Less,
            _ => false,
        }
    }
    fn __hash__(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        self.inner.to_string().hash(&mut h);
        h.finish()
    }
}

#[pyclass(name = "SimpleSpec")]
#[derive(Clone)]
struct PySimpleSpec {
    inner: RSimpleSpec,
}

#[pymethods]
impl PySimpleSpec {
    #[new]
    fn new(expression: &str) -> PyResult<Self> {
        RSimpleSpec::parse(expression)
            .map(|inner| PySimpleSpec { inner })
            .map_err(|e| PyValueError::new_err(e.to_string()))
    }
    fn match_(&self, version: &PyVersion) -> bool {
        self.inner.matches(&version.inner)
    }
    fn __contains__(&self, version: &PyVersion) -> bool {
        self.inner.matches(&version.inner)
    }
    fn __repr__(&self) -> String {
        format!("SimpleSpec('{}')", self.inner.expression)
    }
}

#[pyclass(name = "NpmSpec")]
#[derive(Clone)]
struct PyNpmSpec {
    inner: RNpmSpec,
}

#[pymethods]
impl PyNpmSpec {
    #[new]
    fn new(expression: &str) -> PyResult<Self> {
        RNpmSpec::parse(expression)
            .map(|inner| PyNpmSpec { inner })
            .map_err(|e| PyValueError::new_err(e.to_string()))
    }
    fn __contains__(&self, version: &PyVersion) -> bool {
        self.inner.matches(&version.inner)
    }
    #[getter]
    fn clause(&self) -> PyClause {
        PyClause { inner: self.inner.clause.clone() }
    }
    fn __repr__(&self) -> String {
        format!("NpmSpec('{}')", self.inner.expression)
    }
}

/// Exposed only so `.clause` is inspectable/comparable from Python.
/// NOTE: equality reflects construction structure, not full logical
/// simplification — see DECISIONS.md scope cut.
#[pyclass(name = "Clause")]
#[derive(Clone, PartialEq)]
struct PyClause {
    inner: RClause,
}

#[pymethods]
impl PyClause {
    fn __eq__(&self, other: &PyClause) -> bool {
        self.inner == other.inner
    }
    fn __repr__(&self) -> String {
        format!("{:?}", self.inner)
    }
}

#[pyfunction]
fn compare(a: &str, b: &str) -> PyResult<i32> {
    let va = RVersion::parse(a).map_err(|e| PyValueError::new_err(format!("{:?}", e)))?;
    let vb = RVersion::parse(b).map_err(|e| PyValueError::new_err(format!("{:?}", e)))?;
    Ok(match va.sort_key().cmp(&vb.sort_key()) {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    })
}

#[pyfunction]
fn match_(spec: &str, version: &str) -> PyResult<bool> {
    let s = RSimpleSpec::parse(spec).map_err(|e| PyValueError::new_err(e.to_string()))?;
    let v = RVersion::parse(version).map_err(|e| PyValueError::new_err(format!("{:?}", e)))?;
    Ok(s.matches(&v))
}

#[pyfunction]
fn validate(version: &str) -> bool {
    RVersion::parse(version).is_ok()
}

fn register_common(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyVersion>()?;
    m.add_class::<PySpecItem>()?;
    m.add_class::<PySimpleSpec>()?;
    m.add("Spec", m.getattr("SimpleSpec")?)?; // historical alias
    m.add_class::<PyNpmSpec>()?;
    m.add_class::<PyClause>()?;
    m.add_function(wrap_pyfunction!(compare, m)?)?;
    m.add_function(wrap_pyfunction!(match_, m)?)?;
    m.add_function(wrap_pyfunction!(validate, m)?)?;
    Ok(())
}

#[pymodule]
fn semantic_version(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    register_common(m)?;
    // Submodule so `from semantic_version import base` works, matching
    // the original library's own package layout.
    let base = PyModule::new_bound(py, "base")?;
    register_common(&base)?;
    m.add_submodule(&base)?;
    py.import_bound("sys")?
        .getattr("modules")?
        .set_item("semantic_version.base", base)?;
    Ok(())
}

use crate::spec_item::SpecItem as RSpecItem;

#[pyclass(name = "SpecItem")]
#[derive(Clone)]
struct PySpecItem {
    inner: RSpecItem,
}

#[pymethods]
impl PySpecItem {
    #[new]
    fn new(expression: &str) -> PyResult<Self> {
        RSpecItem::parse(expression)
            .map(|inner| PySpecItem { inner })
            .map_err(PyValueError::new_err)
    }

    fn __eq__(&self, other: &PySpecItem) -> bool {
        self.inner == other.inner
    }
    fn __hash__(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        self.inner.hash(&mut h);
        h.finish()
    }

    #[classattr]
    const KIND_ANY: &'static str = "*";
    #[classattr]
    const KIND_LT: &'static str = "<";
    #[classattr]
    const KIND_LTE: &'static str = "<=";
    #[classattr]
    const KIND_EQUAL: &'static str = "==";
    #[classattr]
    const KIND_GTE: &'static str = ">=";
    #[classattr]
    const KIND_GT: &'static str = ">";
    #[classattr]
    const KIND_NEQ: &'static str = "!=";
    #[classattr]
    const KIND_CARET: &'static str = "^";
    #[classattr]
    const KIND_TILDE: &'static str = "~";

    #[getter]
    fn kind(&self) -> &'static str {
        self.inner.kind.as_str()
    }

    #[getter]
    fn spec(&self) -> PyVersion {
        PyVersion { inner: crate::version::Version {
            major: self.inner.major,
            minor: self.inner.minor.unwrap_or(0),
            patch: self.inner.patch.unwrap_or(0),
            prerelease: self.inner.prerelease.clone().unwrap_or_default(),
            build: self.inner.build.clone().unwrap_or_default(),
        }}
    }

    fn match_(&self, version: &PyVersion) -> bool {
        self.inner.matches(&version.inner)
    }

    fn __str__(&self) -> String {
        self.inner.to_display_string()
    }
    fn __repr__(&self) -> String {
        format!("<SpecItem: {}>", self.inner.to_display_string())
    }
}
