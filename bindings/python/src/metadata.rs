//! Typed metadata constructors translated to portable strategy declarations.
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyType;
use riff_core::{ConstraintMode, Source, Strategy};
use serde_json::{Map, Value};

use crate::json::{JsonInput, ListInput, MappingInput, SequenceInput, to_python};

/// Generate from schema constraints; violate is reserved for failures.
#[pyclass(frozen, module = "riff._native")]
pub struct SchemaStrategy {
    constraints: ConstraintMode,
}
/// Choose from a nonempty explicit corpus.
#[pyclass(frozen, module = "riff._native")]
pub struct ValuesStrategy {
    values: Vec<Value>,
}
/// Invoke a registered semantic provider with isolated options.
#[pyclass(frozen, module = "riff._native")]
pub struct ProviderStrategy {
    name: String,
    locale: String,
    options: Map<String, Value>,
}
/// Choose among child strategies.
#[pyclass(frozen, module = "riff._native")]
pub struct ChoiceStrategy {
    strategies: Vec<Strategy>,
}
/// Reject child values that fail a schema predicate.
#[pyclass(frozen, module = "riff._native")]
pub struct FilterStrategy {
    strategy: Strategy,
    predicate: Value,
}

#[derive(FromPyObject)]
pub enum StrategyInput<'py> {
    Schema(PyRef<'py, SchemaStrategy>),
    Values(PyRef<'py, ValuesStrategy>),
    Provider(PyRef<'py, ProviderStrategy>),
    Choice(PyRef<'py, ChoiceStrategy>),
    Filter(PyRef<'py, FilterStrategy>),
}

impl StrategyInput<'_> {
    fn into_core(self) -> Strategy {
        match self {
            Self::Schema(value) => Strategy::Schema { constraints: value.constraints },
            Self::Values(value) => Strategy::Values { values: value.values.clone() },
            Self::Provider(value) => Strategy::Provider {
                name: value.name.clone(),
                locale: value.locale.clone(),
                options: value.options.clone(),
            },
            Self::Choice(value) => Strategy::Choice { strategies: value.strategies.clone() },
            Self::Filter(value) => Strategy::Filter {
                schema: value.predicate.clone(),
                strategy: Box::new(value.strategy.clone()),
            },
        }
    }
}

fn kind(actual: &str, expected: &str) -> PyResult<()> {
    if actual != expected {
        return Err(PyValueError::new_err(format!("Expected strategy kind {expected:?}.")));
    }
    Ok(())
}

fn strategy_object(py: Python<'_>, strategy: &Strategy) -> PyResult<Py<PyAny>> {
    match strategy {
        Strategy::Schema { constraints } => {
            Ok(Py::new(py, SchemaStrategy { constraints: *constraints })?.into_any())
        }
        Strategy::Values { values } => {
            Ok(Py::new(py, ValuesStrategy { values: values.clone() })?.into_any())
        }
        Strategy::Provider { name, locale, options } => Ok(Py::new(
            py,
            ProviderStrategy {
                name: name.clone(),
                locale: locale.clone(),
                options: options.clone(),
            },
        )?
        .into_any()),
        Strategy::Choice { strategies } => {
            Ok(Py::new(py, ChoiceStrategy { strategies: strategies.clone() })?.into_any())
        }
        Strategy::Filter { schema, strategy } => Ok(Py::new(
            py,
            FilterStrategy { predicate: schema.clone(), strategy: (**strategy).clone() },
        )?
        .into_any()),
    }
}

#[pymethods]
impl SchemaStrategy {
    #[new]
    #[pyo3(signature = (*, kind, constraints="respect"))]
    fn new(kind: &str, constraints: &str) -> PyResult<Self> {
        self::kind(kind, "schema")?;
        let constraints = serde_json::from_value(Value::String(constraints.to_owned()))
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        Ok(Self { constraints })
    }
    #[classattr]
    #[pyo3(name = "kind")]
    const KIND: &'static str = "schema";
    #[getter]
    const fn constraints(&self) -> &'static str {
        match self.constraints {
            ConstraintMode::Respect => "respect",
            ConstraintMode::Violate => "violate",
        }
    }
}
#[pymethods]
impl ValuesStrategy {
    #[new]
    #[pyo3(signature = (*, kind, values))]
    fn new(kind: &str, values: ListInput<JsonInput>) -> PyResult<Self> {
        self::kind(kind, "values")?;
        Ok(Self { values: values.0.into_iter().map(|value| value.0).collect() })
    }
    #[classattr]
    #[pyo3(name = "kind")]
    const KIND: &'static str = "values";
    #[getter]
    fn values(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        to_python(py, &Value::Array(self.values.clone()))
    }
}
#[pymethods]
impl ProviderStrategy {
    #[new]
    #[pyo3(signature = (*, kind, name, locale="en-US", options=None))]
    fn new(
        kind: &str,
        name: String,
        locale: &str,
        options: Option<MappingInput>,
    ) -> PyResult<Self> {
        self::kind(kind, "provider")?;
        Ok(Self {
            name,
            locale: locale.to_owned(),
            options: options.map(|value| value.0).unwrap_or_default(),
        })
    }
    #[classattr]
    #[pyo3(name = "kind")]
    const KIND: &'static str = "provider";
    #[getter]
    fn name(&self) -> &str {
        &self.name
    }
    #[getter]
    fn locale(&self) -> &str {
        &self.locale
    }
    #[getter]
    fn options(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        to_python(py, &Value::Object(self.options.clone()))
    }
}
#[pymethods]
impl ChoiceStrategy {
    #[new]
    #[pyo3(signature = (*, kind, strategies))]
    fn new(kind: &str, strategies: ListInput<StrategyInput<'_>>) -> PyResult<Self> {
        self::kind(kind, "one-of")?;
        Ok(Self { strategies: strategies.0.into_iter().map(StrategyInput::into_core).collect() })
    }
    #[classattr]
    #[pyo3(name = "kind")]
    const KIND: &'static str = "one-of";
    #[getter]
    fn strategies(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.strategies.iter().map(|strategy| strategy_object(py, strategy)).collect()
    }
}
#[pymethods]
impl FilterStrategy {
    #[new]
    #[pyo3(signature = (*, kind, strategy, schema))]
    fn new(kind: &str, strategy: StrategyInput<'_>, schema: JsonInput) -> PyResult<Self> {
        self::kind(kind, "filter")?;
        if !schema.0.is_object() && !schema.0.is_boolean() {
            return Err(PyValueError::new_err("Predicate schema must be an object or boolean."));
        }
        Ok(Self { strategy: strategy.into_core(), predicate: schema.0 })
    }
    #[classmethod]
    #[pyo3(signature = (strategy, *, schema))]
    fn r#where(
        _cls: &Bound<'_, PyType>,
        strategy: StrategyInput<'_>,
        schema: JsonInput,
    ) -> PyResult<Self> {
        Self::new("filter", strategy, schema)
    }
    #[classattr]
    #[pyo3(name = "kind")]
    const KIND: &'static str = "filter";
    #[getter]
    fn strategy(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        strategy_object(py, &self.strategy)
    }
    #[getter]
    fn predicate(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        to_python(py, &self.predicate)
    }
}

/// Declare a negative source and an optional consumer-facing reason.
#[pyclass(frozen, module = "riff._native")]
pub struct Failure {
    source: Source,
}

#[pymethods]
impl Failure {
    #[new]
    #[pyo3(signature = (*, provider=None, locale=None, options=None, examples=None, strategy=None, reason=None))]
    fn new(
        provider: Option<String>,
        locale: Option<String>,
        options: Option<MappingInput>,
        examples: Option<SequenceInput>,
        strategy: Option<StrategyInput<'_>>,
        reason: Option<String>,
    ) -> Self {
        Self {
            source: Source {
                provider,
                locale,
                options: options.map(|value| value.0),
                examples: examples.map(|value| value.0),
                strategy: strategy.map(StrategyInput::into_core),
                reason,
            },
        }
    }
    #[getter]
    fn provider(&self) -> Option<&str> {
        self.source.provider.as_deref()
    }
    #[getter]
    fn locale(&self) -> &str {
        self.source.locale.as_deref().unwrap_or("en-US")
    }
    #[getter]
    fn options(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        to_python(py, &Value::Object(self.source.options.clone().unwrap_or_default()))
    }
    #[getter]
    fn examples(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        self.source
            .examples
            .as_ref()
            .map(|value| to_python(py, &Value::Array(value.clone())))
            .transpose()
    }
    #[getter]
    fn strategy(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        self.source.strategy.as_ref().map(|value| strategy_object(py, value)).transpose()
    }
    #[getter]
    fn reason(&self) -> Option<&str> {
        self.source.reason.as_deref()
    }
}

#[pyfunction]
#[pyo3(signature = (*, provider=None, locale=None, options=None, examples=None, strategy=None, fail=None))]
fn fixture(
    py: Python<'_>,
    provider: Option<String>,
    locale: Option<String>,
    options: Option<MappingInput>,
    examples: Option<SequenceInput>,
    strategy: Option<StrategyInput<'_>>,
    fail: Option<PyRef<'_, Failure>>,
) -> PyResult<Py<PyAny>> {
    let source = Source {
        provider,
        locale,
        options: options.map(|value| value.0),
        examples: examples.map(|value| value.0),
        strategy: strategy.map(StrategyInput::into_core),
        reason: None,
    };
    let mut metadata =
        serde_json::to_value(source).map_err(|error| PyValueError::new_err(error.to_string()))?;
    if let Some(fail) = fail {
        let failure = serde_json::to_value(&fail.source)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        let Value::Object(ref mut object) = metadata else {
            return Err(PyValueError::new_err("Metadata must be an object."));
        };
        let _ = object.insert("fail".to_owned(), failure);
    }
    let _ = riff_core::Metadata::parse(Some(&metadata))
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
    to_python(py, &serde_json::json!({"x-fixture":metadata}))
}

pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<SchemaStrategy>()?;
    module.add_class::<ValuesStrategy>()?;
    module.add_class::<ProviderStrategy>()?;
    module.add_class::<ChoiceStrategy>()?;
    module.add_class::<FilterStrategy>()?;
    module.add_class::<Failure>()?;
    module.add_function(wrap_pyfunction!(fixture, module)?)?;
    Ok(())
}
