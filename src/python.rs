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
use pyo3::types::{PyType, PyTuple};
use std::cmp::Ordering;

use crate::spec::{Clause as RClause, SimpleSpec as RSimpleSpec};
use crate::npm_spec::NpmSpec as RNpmSpec;
use crate::version::Version as RVersion;

#[pyclass(name = "Version", subclass)]
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
    #[pyo3(signature = (level="patch"))]
    fn truncate(&self, level: &str) -> PyResult<PyVersion> {
        let v = &self.inner;
        let result = match level {
            "major" => RVersion { major: v.major, minor: 0, patch: 0, prerelease: vec![], build: vec![] },
            "minor" => RVersion { major: v.major, minor: v.minor, patch: 0, prerelease: vec![], build: vec![] },
            "patch" => RVersion { major: v.major, minor: v.minor, patch: v.patch, prerelease: vec![], build: vec![] },
            "prerelease" => RVersion { major: v.major, minor: v.minor, patch: v.patch, prerelease: v.prerelease.clone(), build: vec![] },
            "build" => v.clone(),
            other => return Err(PyValueError::new_err(format!("Invalid truncation level: {:?}", other))),
        };
        Ok(PyVersion { inner: result })
    }

    #[getter]
    fn major(&self) -> u64 { self.inner.major }
    #[getter]
    fn minor(&self) -> u64 { self.inner.minor }
    #[getter]
    fn patch(&self) -> u64 { self.inner.patch }
    #[getter]
    fn prerelease(&self, py: Python<'_>) -> Py<PyTuple> {
        PyTuple::new_bound(py, self.inner.prerelease.iter().map(|s| s.as_str())).into()
    }
    #[getter]
    fn build(&self, py: Python<'_>) -> Py<PyTuple> {
        PyTuple::new_bound(py, self.inner.build.iter().map(|s| s.as_str())).into()
    }
    #[getter]
    fn precedence_key(&self, py: Python<'_>) -> PyObject {
        use pyo3::types::PyTuple;
        let v = &self.inner;
        let prerelease_rank: i64 = if v.prerelease.is_empty() { 1 } else { 0 };
        let prerelease_items: Vec<PyObject> = v.prerelease.iter().map(|s| {
            if let Ok(n) = s.parse::<u64>() {
                PyTuple::new_bound(py, &[0i64.into_py(py), n.into_py(py)]).into_py(py)
            } else {
                PyTuple::new_bound(py, &[1i64.into_py(py), s.clone().into_py(py)]).into_py(py)
            }
        }).collect();
        PyTuple::new_bound(py, &[
            v.major.into_py(py),
            v.minor.into_py(py),
            v.patch.into_py(py),
            prerelease_rank.into_py(py),
            prerelease_items.into_py(py),
            v.build.clone().into_py(py),
        ]).into_py(py)
    }

    fn __str__(&self) -> String {
        self.inner.to_string()
    }
    fn __repr__(&self) -> String {
        format!("Version('{}')", self.inner)
    }
    fn __richcmp__(&self, other: &PyVersion, op: pyo3::basic::CompareOp) -> bool {
        use pyo3::basic::CompareOp::*;
        let ord = self.inner.partial_cmp(&other.inner);
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
    #[pyo3(signature = (*parts))]
    fn new(parts: Vec<String>) -> PyResult<Self> {
        let expression = parts.join(",");
        RSimpleSpec::parse(&expression)
            .map(|inner| PySimpleSpec { inner })
            .map_err(|e| PyValueError::new_err(e.to_string()))
    }

    #[pyo3(name = "match")]
    fn match_(&self, version: &PyVersion) -> bool {
        self.inner.matches(&version.inner)
    }

    fn __contains__(&self, item: &Bound<'_, PyAny>) -> bool {
        let version = if let Ok(v) = item.extract::<PyVersion>() {
            v.inner
        } else if let Ok(s) = item.extract::<String>() {
            match RVersion::parse(&s) {
                Ok(v) => v,
                Err(_) => return false,
            }
        } else {
            return false;
        };
        self.inner.matches(&version)
    }

    fn filter(&self, versions: Vec<PyVersion>) -> Vec<PyVersion> {
        versions.into_iter().filter(|v| self.inner.matches(&v.inner)).collect()
    }

    fn select(&self, versions: Vec<PyVersion>) -> Option<PyVersion> {
        let refs: Vec<&RVersion> = versions.iter().map(|v| &v.inner).collect();
        self.inner.select(refs).map(|v| PyVersion { inner: v.clone() })
    }

    fn __eq__(&self, other: &PySimpleSpec) -> bool {
        self.inner.expression == other.inner.expression
    }
    fn __hash__(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        self.inner.expression.hash(&mut h);
        h.finish()
    }

    fn __str__(&self) -> String {
        self.inner.expression.clone()
    }
    fn __repr__(&self) -> String {
        format!("SimpleSpec('{}')", self.inner.expression)
    }
    fn __iter__(&self, py: Python<'_>) -> PyResult<PyObject> {
        fn op_str(op: &crate::spec::Operator) -> &'static str {
            match op {
                crate::spec::Operator::Eq => "==",
                crate::spec::Operator::Neq => "!=",
                crate::spec::Operator::Lt => "<",
                crate::spec::Operator::Lte => "<=",
                crate::spec::Operator::Gt => ">",
                crate::spec::Operator::Gte => ">=",
            }
        }
        fn collect(clause: &crate::spec::Clause, out: &mut Vec<String>) {
            match clause {
                crate::spec::Clause::Always | crate::spec::Clause::Never => {}
                crate::spec::Clause::Range(r) => {
                    out.push(format!("{}{}", op_str(&r.operator), r.target));
                }
                crate::spec::Clause::AllOf(v) | crate::spec::Clause::AnyOf(v) => {
                    for c in v {
                        collect(c, out);
                    }
                }
            }
        }
        let mut parts = Vec::new();
        collect(&self.inner.clause, &mut parts);
        let list = pyo3::types::PyList::new_bound(py, parts);
        let iter_obj = list.call_method0("__iter__")?;
        Ok(iter_obj.unbind())
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
fn compare(py: Python<'_>, a: &str, b: &str) -> PyResult<PyObject> {
    let va = RVersion::parse(a).map_err(|e| PyValueError::new_err(format!("{:?}", e)))?;
    let vb = RVersion::parse(b).map_err(|e| PyValueError::new_err(format!("{:?}", e)))?;

    // Build metadata has no defined ordering (see DECISIONS.md: Ord vs
    // PartialOrd). If everything but build differs, mirror the original
    // library's behavior of returning NotImplemented rather than a
    // fabricated -1/0/1.
    let a_no_build = va.truncate_prerelease();
    let b_no_build = vb.truncate_prerelease();
    if a_no_build == b_no_build && va.build != vb.build {
        return Ok(py.NotImplemented());
    }

    let result = match va.sort_key().cmp(&vb.sort_key()) {
        Ordering::Less => -1i32,
        Ordering::Equal => 0i32,
        Ordering::Greater => 1i32,
    };
    Ok(result.into_py(py))
}

#[pyfunction]
#[pyo3(name = "match")]
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
    m.add_class::<PySpecTarget>()?;
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

/// Exposes SpecItem's raw components, preserving None (unmentioned in the
/// pattern) vs Some(vec) (explicitly given, possibly empty) — a distinction
/// a fully-resolved Version can't represent.
#[pyclass(name = "_SpecTarget")]
#[derive(Clone)]
struct PySpecTarget {
    major: u64,
    minor: Option<u64>,
    patch: Option<u64>,
    prerelease: Option<Vec<String>>,
    build: Option<Vec<String>>,
}

#[pymethods]
impl PySpecTarget {
    #[getter] fn major(&self) -> u64 { self.major }
    #[getter] fn minor(&self) -> Option<u64> { self.minor }
    #[getter] fn patch(&self) -> Option<u64> { self.patch }
    #[getter]
    fn prerelease(&self, py: Python<'_>) -> PyObject {
        match &self.prerelease {
            None => py.None(),
            Some(v) => PyTuple::new_bound(py, v.iter().map(|s| s.as_str())).into_py(py),
        }
    }
    #[getter]
    fn build(&self, py: Python<'_>) -> PyObject {
        match &self.build {
            None => py.None(),
            Some(v) => PyTuple::new_bound(py, v.iter().map(|s| s.as_str())).into_py(py),
        }
    }
}

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
    fn spec(&self) -> PySpecTarget {
        PySpecTarget {
            major: self.inner.major,
            minor: self.inner.minor,
            patch: self.inner.patch,
            prerelease: self.inner.prerelease.clone(),
            build: self.inner.build.clone(),
        }
    }

    #[pyo3(name = "match")]
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
