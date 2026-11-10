//! Typed Python views of core results and callback contexts.
use pyo3::PyTraverseError;
use pyo3::exceptions::PyValueError;
use pyo3::gc::PyVisit;
use pyo3::prelude::*;
use pyo3::types::{PyGenericAlias, PyTuple, PyType};
use serde_json::Value;

use crate::json::{Integer, JsonInput, to_python};

/// One validator diagnostic at an RFC 6901 payload pointer.
#[pyclass(frozen, module = "riff._native", eq)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
    #[pyo3(get)]
    path: String,
    #[pyo3(get)]
    message: String,
}

#[pymethods]
impl Violation {
    #[new]
    #[pyo3(signature = (path, message))]
    const fn new(path: String, message: String) -> Self {
        Self { path, message }
    }
}

/// A validated successful value with its original JSON payload.
#[pyclass(frozen, module = "riff._native")]
pub struct SuccessCase {
    #[pyo3(get)]
    value: Py<PyAny>,
    #[pyo3(get)]
    payload: Py<PyAny>,
}

impl SuccessCase {
    pub fn from_value(py: Python<'_>, value: &Value) -> PyResult<Self> {
        Ok(Self { value: to_python(py, value)?, payload: to_python(py, value)? })
    }
}

#[pymethods]
impl SuccessCase {
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.value)?;
        visit.call(&self.payload)?;
        Ok(())
    }

    #[new]
    #[pyo3(signature = (value, payload))]
    fn new(py: Python<'_>, value: Py<PyAny>, payload: JsonInput) -> PyResult<Self> {
        let JsonInput(payload) = payload;
        Ok(Self { value, payload: to_python(py, &payload)? })
    }
    #[classmethod]
    fn __class_getitem__<'py>(
        cls: &Bound<'py, PyType>,
        item: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyGenericAlias>> {
        PyGenericAlias::new(cls.py(), cls.as_any(), item)
    }
    #[classattr]
    #[pyo3(name = "expectation")]
    const EXPECTATION: &'static str = "pass";
    #[classattr]
    #[pyo3(name = "schema_valid")]
    const SCHEMA_VALID: bool = true;
    #[classattr]
    fn violations(py: Python<'_>) -> Py<PyTuple> {
        PyTuple::empty(py).unbind()
    }
    #[classattr]
    #[pyo3(name = "target")]
    const TARGET: Option<&'static str> = None;
    #[classattr]
    #[pyo3(name = "reason")]
    const REASON: Option<&'static str> = None;
    fn __eq__(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let Ok(other) = other.extract::<PyRef<'_, Self>>() else { return Ok(false) };
        Ok(self.value.bind(py).eq(other.value.bind(py))?
            && self.payload.bind(py).eq(other.payload.bind(py))?)
    }
}

/// A deliberate mutation with observed validity and diagnostics.
#[pyclass(frozen, module = "riff._native")]
pub struct FailureCase {
    #[pyo3(get)]
    value: Py<PyAny>,
    #[pyo3(get)]
    schema_valid: bool,
    violations: Vec<Violation>,
    #[pyo3(get)]
    target: String,
    #[pyo3(get)]
    reason: Option<String>,
}

impl FailureCase {
    pub fn from_case(py: Python<'_>, case: riff_core::Case) -> PyResult<Self> {
        let target =
            case.target.ok_or_else(|| PyValueError::new_err("Failure case has no target."))?;
        Ok(Self {
            value: to_python(py, &case.value)?,
            schema_valid: case.schema_valid,
            violations: case
                .violations
                .into_iter()
                .map(|value| Violation { path: value.path, message: value.message })
                .collect(),
            target,
            reason: case.reason,
        })
    }
}

#[pymethods]
impl FailureCase {
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.value)?;
        Ok(())
    }

    #[new]
    #[pyo3(signature = (value, schema_valid, violations, target, reason=None))]
    fn new(
        py: Python<'_>,
        value: JsonInput,
        schema_valid: bool,
        violations: Vec<PyRef<'_, Violation>>,
        target: String,
        reason: Option<String>,
    ) -> PyResult<Self> {
        let JsonInput(value) = value;
        Ok(Self {
            value: to_python(py, &value)?,
            schema_valid,
            violations: violations.into_iter().map(|value| (*value).clone()).collect(),
            target,
            reason,
        })
    }
    #[getter]
    fn payload(&self, py: Python<'_>) -> Py<PyAny> {
        self.value.clone_ref(py)
    }
    #[classattr]
    #[pyo3(name = "expectation")]
    const EXPECTATION: &'static str = "fail";
    #[getter]
    fn violations<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        let values = self
            .violations
            .iter()
            .cloned()
            .map(|value| Py::new(py, value))
            .collect::<PyResult<Vec<_>>>()?;
        PyTuple::new(py, values)
    }
    fn __eq__(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let Ok(other) = other.extract::<PyRef<'_, Self>>() else { return Ok(false) };
        Ok(self.schema_valid == other.schema_valid
            && self.violations == other.violations
            && self.target == other.target
            && self.reason == other.reason
            && self.value.bind(py).eq(other.value.bind(py))?)
    }
}

/// A retained reproducer, oracle-call count, and exhaustion state.
#[pyclass(frozen, module = "riff._native")]
pub struct MinimizationResult {
    #[pyo3(get)]
    value: Py<PyAny>,
    #[pyo3(get)]
    evaluations: usize,
    #[pyo3(get)]
    budget_exhausted: bool,
}

impl MinimizationResult {
    pub fn from_result(py: Python<'_>, result: &riff_core::MinimizationResult) -> PyResult<Self> {
        Ok(Self {
            value: to_python(py, &result.value)?,
            evaluations: result.evaluations,
            budget_exhausted: result.budget_exhausted,
        })
    }
}

#[pymethods]
impl MinimizationResult {
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.value)?;
        Ok(())
    }

    #[new]
    #[pyo3(signature = (value, evaluations, budget_exhausted))]
    fn new(
        py: Python<'_>,
        value: JsonInput,
        evaluations: usize,
        budget_exhausted: bool,
    ) -> PyResult<Self> {
        let JsonInput(value) = value;
        Ok(Self { value: to_python(py, &value)?, evaluations, budget_exhausted })
    }
    fn __eq__(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let Ok(other) = other.extract::<PyRef<'_, Self>>() else { return Ok(false) };
        Ok(self.evaluations == other.evaluations
            && self.budget_exhausted == other.budget_exhausted
            && self.value.bind(py).eq(other.value.bind(py))?)
    }
}

/// Isolated invocation inputs; use seed instead of global randomness.
#[pyclass(frozen, module = "riff._native")]
pub struct ProviderContext {
    #[pyo3(get)]
    seed: u64,
    #[pyo3(get)]
    locale: String,
    #[pyo3(get)]
    schema: Py<PyAny>,
    #[pyo3(get)]
    options: Py<PyAny>,
}

impl ProviderContext {
    pub fn from_context(py: Python<'_>, context: &riff_core::ProviderContext) -> PyResult<Self> {
        Ok(Self {
            seed: context.seed,
            locale: context.locale.clone(),
            schema: to_python(py, &context.schema)?,
            options: to_python(py, &Value::Object(context.options.clone()))?,
        })
    }
}

#[pymethods]
impl ProviderContext {
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.schema)?;
        visit.call(&self.options)?;
        Ok(())
    }

    #[new]
    #[pyo3(signature = (seed, locale, schema, options))]
    fn new(
        py: Python<'_>,
        seed: Integer,
        locale: String,
        schema: crate::json::MappingInput,
        options: crate::json::MappingInput,
    ) -> PyResult<Self> {
        let Integer(seed) = seed;
        let crate::json::MappingInput(schema) = schema;
        let crate::json::MappingInput(options) = options;
        let seed = seed.as_u64().ok_or_else(|| {
            PyValueError::new_err("Provider seeds must be unsigned 64-bit integers.")
        })?;
        Ok(Self {
            seed,
            locale,
            schema: to_python(py, &Value::Object(schema))?,
            options: to_python(py, &Value::Object(options))?,
        })
    }
}

pub fn case_object(py: Python<'_>, case: riff_core::Case) -> PyResult<Py<PyAny>> {
    match case.expectation.as_str() {
        "pass" => Ok(Py::new(py, SuccessCase::from_value(py, &case.value)?)?.into_any()),
        "fail" => Ok(Py::new(py, FailureCase::from_case(py, case)?)?.into_any()),
        _ => Err(PyValueError::new_err("Unknown fixture expectation.")),
    }
}
